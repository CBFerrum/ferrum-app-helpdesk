//! A reply on its way to the customer: the email it becomes, and the files it carries.
//!
//! Where the Mail app is installed the reply goes through `ferrum_app_comms::outbound::send`, on the ticket: with
//! In-Reply-To and References from the conversation's own message ids, the ticket's CC, and a Reply-To that is the
//! support address with the ticket's secret in it (`support+<secret>@acme.test`), so the customer's answer is matched
//! to the ticket by the headers it carries, then by the address it was written to. The Message-ID Mail gave the
//! message is kept on the conversation row. Without Mail it goes through the platform's mail
//! (`Kernel::queue_mail`), one HTML message to each address with no threading headers. Either way the subject
//! carries the ticket's number and secret (`[TKT-2026-000123/…]`), the last thing an answer is matched by when
//! nothing else does, and the wording around the agent's words is the message template `helpdesk.reply`, which
//! the workspace edits. Files are the platform's stored files: an agent uploads one (`POST /api/files`), then
//! names it in the message, and the platform attaches it to the ticket (`Kernel::attach_files_tx`).

use ferrum_domain::util::{b, id, rows, s, sys};
use ferrum_kernel::{Ctx, Doc, Error, Filter, HookCtx, Kernel, Result};
use serde_json::{Value, json};
use uuid::Uuid;

/// Most files one message carries, and their size together: what one queued email can attach.
pub const MAX_FILES: usize = 10;
pub const MAX_BYTES: i64 = 20 << 20;

/// The longest subject the platform's mail takes.
const MAX_SUBJECT: usize = 300;

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// "Re: [TKT-2026-000123/secret] Printer jam": the subject the template `helpdesk.reply` gives by default. The
/// number and the secret say which ticket it is about, so an answer finds it even when its headers do not.
pub fn subject(number: &str, token: &str, subject: &str) -> String {
    let mut out = format!("Re: [{number}/{token}] ");
    for c in subject.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > MAX_SUBJECT {
            break;
        }
        out.push(c);
    }
    out
}

fn is_web(url: &str) -> bool {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.contains(char::is_whitespace))
}

/// The text of a bullet-list line ("- one", "* one").
fn bullet(line: &str) -> Option<String> {
    let l = line.trim_start();
    l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).map(str::to_string)
}

/// The inline Markdown agents use, in text that is already escaped: `code`, **bold**, [links](https://…) and
/// bare web addresses. Nothing else is turned into markup.
fn inline(text: &str) -> String {
    let e = escape(text);
    let mut out = String::with_capacity(e.len());
    let mut i = 0;
    while i < e.len() {
        let rest = &e[i..];
        if let Some(r) = rest.strip_prefix('`')
            && let Some(end) = r.find('`').filter(|&n| n > 0)
        {
            out.push_str(&format!("<code>{}</code>", &r[..end]));
            i += end + 2;
            continue;
        }
        if let Some(r) = rest.strip_prefix("**")
            && let Some(end) = r.find("**").filter(|&n| n > 0)
        {
            out.push_str(&format!("<strong>{}</strong>", &r[..end]));
            i += end + 4;
            continue;
        }
        if let Some(r) = rest.strip_prefix('[')
            && let Some(mid) = r.find("](").filter(|&n| n > 0)
            && let Some(close) = r[mid + 2..].find(')')
            && is_web(&r[mid + 2..mid + 2 + close])
        {
            out.push_str(&format!("<a href=\"{}\">{}</a>", &r[mid + 2..mid + 2 + close], &r[..mid]));
            i += mid + close + 4;
            continue;
        }
        let at_word = e[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        if at_word && (rest.starts_with("https://") || rest.starts_with("http://")) {
            let word = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
            let url = word.trim_end_matches(['.', ',', ':', '!', '?', ')']);
            if is_web(url) {
                out.push_str(&format!("<a href=\"{url}\">{url}</a>"));
                i += url.len();
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        i += c.len_utf8().max(1);
    }
    out
}

/// The Markdown agents write, as HTML a mail client shows: paragraphs, line breaks, bullet lists and the
/// inline marks above. Text is escaped first, so no tag anyone typed survives.
pub fn markdown_html(src: &str) -> String {
    let mut blocks: Vec<Vec<&str>> = vec![vec![]];
    for line in src.lines() {
        if line.trim().is_empty() {
            if blocks.last().is_some_and(|b| !b.is_empty()) {
                blocks.push(vec![]);
            }
        } else if let Some(b) = blocks.last_mut() {
            b.push(line.trim_end());
        }
    }
    let mut out = String::new();
    for lines in blocks.iter().filter(|b| !b.is_empty()) {
        if lines.iter().all(|l| bullet(l).is_some()) {
            out.push_str("<ul style=\"margin:0 0 12px;padding-left:20px\">");
            for l in lines {
                out.push_str(&format!("<li>{}</li>", inline(&bullet(l).unwrap_or_default())));
            }
            out.push_str("</ul>");
        } else {
            out.push_str("<p style=\"margin:0 0 12px\">");
            out.push_str(&lines.iter().map(|l| inline(l)).collect::<Vec<_>>().join("<br>"));
            out.push_str("</p>");
        }
    }
    out
}

/// Where links in the emails point: the workspace's own address when the server is reached over HTTPS (or
/// does not say), else the server's own (`PUBLIC_URL`); none without either. The same rule the
/// platform's notification emails follow.
pub async fn link_base(k: &Kernel, tenant: Uuid) -> Option<String> {
    let public = k.settings.public_url.clone();
    if public.as_deref().is_none_or(|u| u.starts_with("https://"))
        && let Ok(Some(host)) = k.primary_host(tenant, "shell").await
    {
        return Some(format!("https://{host}"));
    }
    public.map(|u| u.trim_end_matches('/').to_string())
}

/// The ids in a message's `attachments` (`[{id, …}]` or `[id]`), each once.
pub fn file_ids(v: Option<&Value>) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = Vec::new();
    for x in v.and_then(Value::as_array).into_iter().flatten() {
        if let Some(id) = x.get("id").and_then(Value::as_str).or(x.as_str()).and_then(|s| s.parse().ok())
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

/// The files a new message names, attached to the ticket, as the stored files describe them (what the client wrote
/// about a file's name or size is not believed). Each must be a file this person uploaded and attached to nothing
/// yet, or one already on this ticket; at most [`MAX_FILES`] and [`MAX_BYTES`] together.
pub async fn check_files(hc: &mut HookCtx<'_>, ticket: Uuid, wanted: &Value) -> Result<Vec<Value>> {
    if !wanted.is_array() {
        return Err(Error::field("attachments", "must be a list of the files uploaded"));
    }
    let ids = file_ids(Some(wanted));
    if wanted.as_array().is_some_and(|a| a.len() != ids.len()) {
        return Err(Error::field("attachments", "each file is named by the id it was uploaded with, once"));
    }
    if ids.len() > MAX_FILES {
        return Err(Error::field("attachments", format!("at most {MAX_FILES} files in one message")));
    }
    if ids.is_empty() {
        return Ok(vec![]);
    }
    // The platform asks what attaching asks (whoever may change the ticket), accepts only this ticket's own files and the
    // caller's loose uploads, and attaches the loose ones to the ticket.
    // (Its refusals name the field `files`; the message's own is `attachments`.)
    let described = hc.kernel.attach_files_tx(hc.ctx, hc.tx, &ids, "Ticket", ticket).await.map_err(|e| match e {
        Error::Validation(fields) => Error::Validation(fields.into_iter().map(|f| ferrum_kernel::FieldError { field: "attachments".into(), ..f }).collect()),
        // A ticket being created has no record to attach to yet.
        Error::NotFound(_) => Error::field("attachments", "can be added once the ticket exists"),
        other => other,
    })?;
    let total: i64 = described.iter().filter_map(|f| f.get("size").and_then(Value::as_i64)).sum();
    if total > MAX_BYTES {
        return Err(Error::field("attachments", format!("files are limited to {} MB together", MAX_BYTES >> 20)));
    }
    Ok(described)
}

/// Renders the message template `helpdesk.reply` (the workspace's copy, else the package's) for a ticket and
/// the values it declares: the mail's subject and HTML.
async fn render_reply(k: &Kernel, sctx: &Ctx, ticket: &Doc, values: &Value) -> Result<(String, String)> {
    let def = k.message_template(sctx, "helpdesk.reply").await?;
    let languages = def["template"]["languages"].as_object().ok_or_else(|| Error::internal("the template helpdesk.reply has no languages"))?;
    let email = languages
        .get("en")
        .or_else(|| languages.values().next())
        .and_then(|l| l.get("email"))
        .ok_or_else(|| Error::internal("the template helpdesk.reply has no email"))?;
    let mut doc = ticket.clone();
    doc.remove("conversation");
    let vars = json!({"doc": doc, "values": values});
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|name| if name == "body" { minijinja::AutoEscape::Html } else { minijinja::AutoEscape::None });
    let bad = |e: minijinja::Error| Error::internal(format!("the template helpdesk.reply: {e}"));
    env.add_template_owned("subject", email["subject"].as_str().unwrap_or_default().to_string()).map_err(bad)?;
    env.add_template_owned("body", email["body"].as_str().unwrap_or_default().to_string()).map_err(bad)?;
    let subject: String =
        env.get_template("subject").and_then(|t| t.render(&vars)).map_err(bad)?.chars().filter(|c| !c.is_control()).take(MAX_SUBJECT).collect();
    let html = env.get_template("body").and_then(|t| t.render(&vars)).map_err(bad)?;
    Ok((subject, html))
}

/// The text of Markdown as plain lines, for the mail's plain part.
fn plain(markdown: &str) -> String {
    markdown.lines().map(|l| l.trim_end().replace("**", "").replace('`', "")).collect::<Vec<_>>().join("\n")
}

/// After a ticket is saved: sends the replies that
/// were marked `emailed` when they were checked, to the ticket's contact (and its CC), through Mail when it is
/// installed. Done in the save's own transaction, so a reply that is refused sends nothing.
pub async fn dispatch(hc: &mut HookCtx<'_>, doc: &Doc, ticket: Uuid, fresh: &[Doc]) -> Result<()> {
    let Some(to) = s(doc, "contact_email").filter(|_| fresh.iter().any(|m| b(m, "emailed"))) else { return Ok(()) };
    let sctx = sys(hc.ctx);
    let number = s(doc, "number").unwrap_or("Ticket");
    let link = link_base(hc.kernel, hc.ctx.tenant).await.map(|base| format!("{base}/open/Ticket/{ticket}"));
    let via_mail = hc.ctx.schema.entity("Communication").is_some();
    let can_attach = via_mail || hc.kernel.mailer.as_ref().is_some_and(|m| m.can_attach());
    // Never to a support address itself: that would answer ourselves.
    let own: Vec<String> =
        hc.kernel.find_tx(&sctx, hc.tx, "SupportAddress", &[], None, 1000).await?.iter().filter_map(|a| s(a, "address").map(crate::inbound::norm)).collect();
    if own.contains(&crate::inbound::norm(to)) {
        return Ok(());
    }
    let copies: Vec<String> = crate::inbound::addresses(s(doc, "cc")).into_iter().filter(|a| !own.contains(a)).collect();
    // Mail with what a send needs (0.20 and later); an older one carries the reply without threading, as the platform's mail does.
    let through_mail = via_mail && hc.ctx.schema.entity("Communication").is_some_and(|c| c.field("reply_to_address").is_some());
    let address = reply_address(hc, doc).await?;
    let brand = match address.as_ref().and_then(|a| id(a, "brand")) {
        Some(b) => hc.kernel.get_tx(&sctx, hc.tx, "SupportBrand", b, false).await.ok(),
        None => None,
    };
    let brand_name = brand.as_ref().and_then(|b| s(b, "display_name").or(s(b, "brand_name"))).map(str::to_string);
    let account = match address.as_ref().and_then(|a| s(a, "mail_account")).map(str::to_string) {
        Some(name) => hc
            .kernel
            .find_tx(&sctx, hc.tx, "EmailAccount", &[Filter::eq("account_name", name)], None, 1)
            .await
            .ok()
            .and_then(|r| r.first().and_then(|d| id(d, "id"))),
        None => None,
    };
    // Answers come back to the support address with the ticket's secret in it.
    let reply_to =
        if through_mail { address.as_ref().and_then(|a| s(a, "address")).zip(s(doc, "mail_token")).and_then(|(a, t)| crate::mail::tagged(a, t)) } else { None };
    let reply_mailbox = reply_to.as_ref().map(|a| mailbox(brand_name.as_deref(), a));
    let (in_reply_to, references) = threading(doc);
    for m in fresh.iter().filter(|m| b(m, "emailed")) {
        let ids = file_ids(m.get("attachments"));
        let names: Vec<String> = m
            .get("attachments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| f.get("file_name").and_then(Value::as_str).map(str::to_string))
            .collect();
        let body = s(m, "body").unwrap_or_default();
        let agent_signature = match s(m, "author") {
            Some(agent) => hc
                .kernel
                .find_tx(&sctx, hc.tx, "AgentProfile", &[Filter::eq("agent", agent.to_lowercase())], None, 1)
                .await?
                .first()
                .and_then(|p| s(p, "signature").map(markdown_html)),
            None => None,
        };
        // The agent's signature, then the brand's.
        let brand_signature = brand.as_ref().and_then(|b| s(b, "signature")).map(markdown_html);
        let signature = match (agent_signature, brand_signature) {
            (None, None) => None,
            (a, b) => Some(format!("{}{}", a.unwrap_or_default(), b.unwrap_or_default())),
        };
        let values = json!({"body": markdown_html(body), "signature": signature, "link": link, "files": names.join(", "), "attached": can_attach && !ids.is_empty(),
            "brand": brand_name});
        // A template that is missing (a workspace not yet updated) or broken (edited) never stops a reply: the
        // agent's words go out in plain form, with the ticket's tag in the subject.
        let (title, html) = match render_reply(hc.kernel, &sctx, doc, &values).await {
            Ok(done) => done,
            Err(e) => {
                tracing::warn!(ticket = %ticket, "the reply template could not be used: {e}");
                let (token, subject_line) = (s(doc, "mail_token").unwrap_or_default(), s(doc, "subject").unwrap_or_default());
                (subject(number, token, subject_line), format!("{}{}", markdown_html(body), signature.clone().unwrap_or_default()))
            }
        };
        // What the brand puts before and after the reply, outside whatever the template says.
        let html = match &brand {
            Some(b) => format!("{}{html}{}", s(b, "header").map(markdown_html).unwrap_or_default(), s(b, "footer").map(markdown_html).unwrap_or_default()),
            None => html,
        };
        if through_mail {
            let message = ferrum_app_comms::outbound::Message {
                to: vec![to.to_string()],
                cc: copies.clone(),
                subject: title,
                html,
                text: Some(plain(body)),
                reference: Some(("Ticket".into(), ticket)),
                account,
                reply_to: reply_mailbox.clone(),
                in_reply_to: in_reply_to.clone(),
                references: references.clone(),
                attachments: Some(m.get("attachments").cloned().unwrap_or_else(|| json!([]))),
                headers: [("X-Ferrum-Helpdesk".to_string(), number.to_string())].into(),
                ..Default::default()
            };
            // A name Mail will not take in a Reply-To (the checks are Mail's) is left off, the address stays.
            let sent = match ferrum_app_comms::outbound::send(hc.kernel, &sctx, hc.tx, &message).await {
                Err(Error::Invalid(_) | Error::Validation(_)) if reply_mailbox != reply_to => {
                    ferrum_app_comms::outbound::send(hc.kernel, &sctx, hc.tx, &ferrum_app_comms::outbound::Message { reply_to: reply_to.clone(), ..message })
                        .await?
                }
                other => other?,
            };
            // The Message-ID Mail gave it, so the customer's answer is found by it.
            if let Some(row) = id(m, "id") {
                sqlx::query("update e_ticket__conversation set message_id = $1, communication = $2 where tenant_id = $3 and id = $4")
                    .bind(&sent.message_id)
                    .bind(sent.communication.to_string())
                    .bind(hc.ctx.tenant)
                    .bind(row)
                    .execute(&mut **hc.tx)
                    .await?;
            }
        } else {
            // A mailer that cannot attach (the development stub) sends the words and points to the ticket.
            for address in std::iter::once(to).chain(copies.iter().map(String::as_str)) {
                if can_attach && !ids.is_empty() {
                    hc.kernel.queue_mail_with(hc.tx, hc.ctx.tenant, address, &title, &html, &ids).await?;
                } else {
                    hc.kernel.queue_mail(hc.tx, hc.ctx.tenant, address, &title, &html).await?;
                }
            }
        }
    }
    Ok(())
}

/// `Name <address>` for a Reply-To: the name as plain words (letters, digits, spaces, dots, dashes and underscores; nothing
/// that could end the header or the name), else the address alone.
fn mailbox(name: Option<&str>, address: &str) -> String {
    let clean: String = name.unwrap_or_default().chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_')).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() { address.to_string() } else { format!("{clean} <{address}>") }
}

/// The support address a reply is sent from: the one the customer wrote to, else one of the ticket's team, else any enabled one.
async fn reply_address(hc: &mut HookCtx<'_>, doc: &Doc) -> Result<Option<Doc>> {
    let sctx = sys(hc.ctx);
    if let Some(a) = id(doc, "channel_address")
        && let Ok(found) = hc.kernel.get_tx(&sctx, hc.tx, "SupportAddress", a, false).await
        && b(&found, "enabled")
    {
        return Ok(Some(found));
    }
    let all = hc.kernel.find_tx(&sctx, hc.tx, "SupportAddress", &[Filter::eq("enabled", true)], Some("address asc"), 100).await?;
    let team = id(doc, "team");
    Ok(all.iter().find(|a| team.is_some() && id(a, "team") == team).or(all.first()).cloned())
}

/// In-Reply-To and References for a reply: the customer's latest message id (else the latest of any), and the ids of
/// the conversation before it, oldest first, at most twenty; only what Mail takes as a message id.
fn threading(doc: &Doc) -> (Option<String>, Vec<String>) {
    let ids: Vec<(bool, String)> = rows(doc, "conversation")
        .iter()
        .filter_map(|m| Some((s(m, "author_kind") == Some("Customer"), ferrum_app_comms::outbound::message_id(s(m, "message_id")?).ok()?)))
        .collect();
    let latest = ids.iter().rev().find(|(customer, _)| *customer).or(ids.last()).map(|(_, m)| m.clone());
    let mut chain: Vec<String> = Vec::new();
    for (_, m) in &ids {
        chain.retain(|c| c != m);
        chain.push(m.clone());
    }
    let skip = chain.len().saturating_sub(20);
    (latest, chain.split_off(skip))
}

/// Whether a new public reply goes out by email: the ticket has a contact address, the server has a
/// mailer (or Mail is installed), the platform's mail accepts the address, and nothing restricts writing to it.
/// If not, the reply is still in the conversation, and in the portal.
pub async fn goes_by_email(hc: &mut HookCtx<'_>, existing: bool, contact: Option<&str>, number: &str, title: &str) -> Result<bool> {
    let Some(to) = contact.filter(|_| existing && (hc.kernel.mailer.is_some() || hc.ctx.schema.entity("Communication").is_some())) else { return Ok(false) };
    if ferrum_kernel::outbox::check_mail(to, &subject(number, "x", title)).is_err() {
        return Ok(false);
    }
    hc.kernel.may_contact(hc.tx, hc.ctx.tenant, to).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_names_the_ticket_and_fits_the_platforms_limit() {
        assert_eq!(subject("TKT-2026-000123", "k3j9x2m1", "Printer jam"), "Re: [TKT-2026-000123/k3j9x2m1] Printer jam");
        let long = subject("TKT-2026-000123", "k3j9x2m1", &"é".repeat(400));
        assert!(long.len() <= 300 && long.starts_with("Re: [TKT-2026-000123/k3j9x2m1] é"), "{}", long.len());
        assert!(ferrum_kernel::outbox::check_mail("ada@shop.test", &long).is_ok());
        assert_eq!(subject("T-1", "abcdefgh", "two\nlines"), "Re: [T-1/abcdefgh] twolines");
        assert!(crate::inbound::tag(&subject("T-1", "abcdefgh", "x")).is_some(), "what goes out is what comes back");
    }

    #[test]
    fn markdown_becomes_paragraphs_lists_and_the_inline_marks_and_nothing_else() {
        assert_eq!(
            markdown_html("Hello **Ada**,\nthanks.\n\n\n- one\n- `two`\n\nSee [the guide](https://help.test/a?x=1&y=2)."),
            concat!(
                "<p style=\"margin:0 0 12px\">Hello <strong>Ada</strong>,<br>thanks.</p>",
                "<ul style=\"margin:0 0 12px;padding-left:20px\"><li>one</li><li><code>two</code></li></ul>",
                "<p style=\"margin:0 0 12px\">See <a href=\"https://help.test/a?x=1&amp;y=2\">the guide</a>.</p>"
            )
        );
    }

    #[test]
    fn a_tag_or_a_script_link_a_person_typed_is_text() {
        let html = markdown_html("<script>alert(1)</script> [x](javascript:alert(1)) [y](https://a.test/\"onmouseover=\"x)");
        assert!(!html.contains("<script") && !html.contains("href=\"javascript"), "{html}");
        assert!(html.contains("&lt;script&gt;") && html.contains("[x](javascript:alert(1))"), "{html}");
        assert!(!html.contains("onmouseover=\""), "{html}");
    }

    #[test]
    fn bare_addresses_become_links_without_their_closing_punctuation() {
        assert_eq!(inline("Go to https://help.test/reset."), "Go to <a href=\"https://help.test/reset\">https://help.test/reset</a>.");
        assert_eq!(inline("not-a-link xhttps://nope"), "not-a-link xhttps://nope");
        assert_eq!(inline("2 * 3 * 4 and a_b_c"), "2 * 3 * 4 and a_b_c");
    }

    #[test]
    fn a_reply_answers_the_customers_latest_message_and_carries_the_chain() {
        let doc = json!({"conversation": [
            {"author_kind": "Customer", "message_id": "c1@cust.test"},
            {"author_kind": "Agent", "message_id": "a1@acme.test"},
            {"author_kind": "Customer", "message_id": "c2@cust.test"},
            {"author_kind": "Agent", "message_id": "a2@acme.test"},
            {"author_kind": "Customer", "message_id": "not an id\r\nBcc: x@y.test"},
            {"author_kind": "Agent", "body": "no id"}]});
        let (reply_to, chain) = threading(doc.as_object().unwrap());
        assert_eq!(reply_to.as_deref(), Some("c2@cust.test"), "the customer's latest message with an id, never header text");
        assert_eq!(chain, ["c1@cust.test", "a1@acme.test", "c2@cust.test", "a2@acme.test"]);
        assert_eq!(threading(json!({"conversation": []}).as_object().unwrap()), (None, vec![]));
        let many: Vec<Value> = (0..30).map(|i| json!({"author_kind": "Agent", "message_id": format!("m{i}@acme.test")})).collect();
        let (last, chain) = threading(json!({"conversation": many}).as_object().unwrap());
        assert_eq!((last.as_deref(), chain.len(), chain[0].as_str()), (Some("m29@acme.test"), 20, "m10@acme.test"));
    }

    #[test]
    fn a_reply_to_names_the_brand_in_plain_words_only() {
        assert_eq!(mailbox(Some("Acme Support"), "support+k3@acme.test"), "Acme Support <support+k3@acme.test>");
        assert_eq!(mailbox(Some("Acme, \"Inc\" <x@y.test>"), "s@acme.test"), "Acme Inc xy.test <s@acme.test>", "nothing that ends a name or a header");
        assert_eq!(mailbox(Some("\r\n"), "s@acme.test"), "s@acme.test");
        assert_eq!(mailbox(None, "s@acme.test"), "s@acme.test");
    }

    #[test]
    fn files_are_named_by_id_once() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        assert_eq!(file_ids(Some(&json!([{"id": a.to_string(), "file_name": "x"}, b.to_string(), {"id": a.to_string()}, {"id": "nonsense"}]))), vec![a, b]);
        assert!(file_ids(None).is_empty() && file_ids(Some(&json!("x"))).is_empty());
    }
}
