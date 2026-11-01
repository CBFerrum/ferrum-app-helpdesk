//! A reply on its way to the customer: the email it becomes, and the files it carries.
//!
//! The platform's mail (`Kernel::queue_mail`) sends one HTML message to one address, with no
//! threading and no reply address, so the subject names the ticket and the message says where to
//! follow it. Files are the platform's stored files: an agent uploads one (`POST /api/files`), then
//! names it in the message.

use ferrum_domain::util::s;
use ferrum_kernel::{Doc, Error, HookCtx, Kernel, Result, Tx};
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

/// "Re: [TKT-2026-000123] Printer jam": the ticket's number is in the subject, so the customer (and a
/// mailbox that files by subject) can tell which ticket it is about.
pub fn subject(number: &str, subject: &str) -> String {
    let mut out = format!("Re: [{number}] ");
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

/// The email for one reply. `files` are the names of what it carries; `attached` says they travel with
/// the email (else the customer is pointed to the ticket for them). `link` is the ticket's address.
pub fn reply_html(body: &str, number: &str, link: Option<&str>, files: &[String], attached: bool) -> String {
    let mut html = String::from(
        "<div style=\"font-family:system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;font-size:15px;line-height:1.5;color:#18181b;max-width:560px\">",
    );
    html.push_str(&markdown_html(body));
    if !files.is_empty() {
        let list = files.iter().map(|f| escape(f)).collect::<Vec<_>>().join(", ");
        let how = match (attached, link) {
            (true, _) => "Attached",
            (false, Some(_)) => "Files with this reply (open the ticket to download them)",
            (false, None) => "Files with this reply",
        };
        html.push_str(&format!("<p style=\"margin:0 0 12px;color:#52525b\">{how}: {list}</p>"));
    }
    let follow = link.map(|l| format!(" · <a href=\"{}\" style=\"color:#52525b\">Follow it, or reply, here</a>", escape(l))).unwrap_or_default();
    html.push_str(&format!(
        "<p style=\"margin:16px 0 0;padding-top:12px;border-top:1px solid #e4e4e7;color:#71717a;font-size:13px\">Ticket {}{follow}</p></div>",
        escape(number)
    ));
    html
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

/// The files a new message names, as the stored files describe them (what the client wrote about a file's
/// name or size is not believed). Each must be a file this person uploaded and attached to nothing yet,
/// or one already on this ticket; at most [`MAX_FILES`] and [`MAX_BYTES`] together.
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
    type Stored = (Uuid, String, String, i64, Option<String>, Option<Uuid>, Option<Uuid>);
    let rows: Vec<Stored> =
        sqlx::query_as("select id, file_name, content_type, size_bytes, entity, record_id, created_by from file_object where tenant_id = $1 and id = any($2)")
            .bind(hc.ctx.tenant)
            .bind(&ids)
            .fetch_all(&mut **hc.tx)
            .await?;
    let (mut out, mut total) = (Vec::new(), 0i64);
    for wanted in &ids {
        let usable = rows.iter().find(|r| {
            r.0 == *wanted
                && match (&r.4, r.5) {
                    (Some(e), Some(rec)) => e == "Ticket" && rec == ticket,
                    _ => hc.ctx.is_system || r.6 == Some(hc.ctx.user),
                }
        });
        let Some((id, name, ctype, size, ..)) = usable else {
            return Err(Error::field("attachments", format!("file {wanted} does not exist, is not yours, or belongs to something else")));
        };
        total += size;
        out.push(json!({"id": id, "file_name": name, "content_type": ctype, "size": size}));
    }
    if total > MAX_BYTES {
        return Err(Error::field("attachments", format!("files are limited to {} MB together", MAX_BYTES >> 20)));
    }
    Ok(out)
}

/// Puts the files that belonged to nothing yet on the ticket, so they list with it and follow its access.
pub async fn attach(tx: &mut Tx, tenant: Uuid, ids: &[Uuid], ticket: Uuid) -> Result<()> {
    if !ids.is_empty() {
        sqlx::query("update file_object set entity = 'Ticket', record_id = $3 where tenant_id = $1 and id = any($2) and entity is null and record_id is null")
            .bind(tenant)
            .bind(ids)
            .bind(ticket)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// After a ticket is saved: puts the files its new messages carry on the ticket, and emails the replies
/// that were stamped with an address (`emailed_to`) when they were checked. Queued in the save's own
/// transaction, so a reply that is refused sends nothing.
pub async fn dispatch(hc: &mut HookCtx<'_>, doc: &Doc, ticket: Uuid, fresh: &[Doc]) -> Result<()> {
    let files: Vec<Uuid> = fresh.iter().flat_map(|m| file_ids(m.get("attachments"))).collect();
    attach(hc.tx, hc.ctx.tenant, &files, ticket).await?;
    if fresh.iter().all(|m| s(m, "emailed_to").is_none()) {
        return Ok(());
    }
    let number = s(doc, "number").unwrap_or("Ticket");
    let title = subject(number, s(doc, "subject").unwrap_or_default());
    let link = link_base(hc.kernel, hc.ctx.tenant).await.map(|base| format!("{base}/open/Ticket/{ticket}"));
    let can_attach = hc.kernel.mailer.as_ref().is_some_and(|m| m.can_attach());
    for m in fresh {
        let Some(to) = s(m, "emailed_to") else { continue };
        let ids = file_ids(m.get("attachments"));
        let names: Vec<String> = m
            .get("attachments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| f.get("file_name").and_then(Value::as_str).map(str::to_string))
            .collect();
        let html = reply_html(s(m, "body").unwrap_or_default(), number, link.as_deref(), &names, can_attach);
        // A mailer that cannot attach (the development stub) sends the words and points to the ticket.
        if can_attach && !ids.is_empty() {
            hc.kernel.queue_mail_with(hc.tx, hc.ctx.tenant, to, &title, &html, &ids).await?;
        } else {
            hc.kernel.queue_mail(hc.tx, hc.ctx.tenant, to, &title, &html).await?;
        }
    }
    Ok(())
}

/// Whom a new public reply is emailed: the ticket's contact, when the server has a mailer, the address
/// is one the platform's mail accepts, and nothing restricts writing to it. Nobody otherwise (the reply
/// is still in the conversation, and in the portal).
pub async fn reply_address(hc: &mut HookCtx<'_>, existing: bool, contact: Option<&str>, number: &str, title: &str) -> Result<Option<String>> {
    let Some(to) = contact.filter(|_| existing && hc.kernel.mailer.is_some()) else { return Ok(None) };
    if ferrum_kernel::outbox::check_mail(to, &subject(number, title)).is_err() || !hc.kernel.may_contact(hc.tx, hc.ctx.tenant, to).await? {
        return Ok(None);
    }
    Ok(Some(to.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_names_the_ticket_and_fits_the_platforms_limit() {
        assert_eq!(subject("TKT-2026-000123", "Printer jam"), "Re: [TKT-2026-000123] Printer jam");
        let long = subject("TKT-2026-000123", &"é".repeat(400));
        assert!(long.len() <= 300 && long.starts_with("Re: [TKT-2026-000123] é"), "{}", long.len());
        assert!(ferrum_kernel::outbox::check_mail("ada@shop.test", &long).is_ok());
        assert_eq!(subject("T-1", "two\nlines"), "Re: [T-1] twolines");
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
    fn the_email_says_where_to_follow_the_ticket_and_what_came_with_it() {
        let html = reply_html("Fixed.", "TKT-1", Some("https://help.test/open/Ticket/abc"), &["fix <1>.pdf".into()], true);
        assert!(
            html.contains("Attached: fix &lt;1&gt;.pdf") && html.contains("Ticket TKT-1") && html.contains("href=\"https://help.test/open/Ticket/abc\""),
            "{html}"
        );
        let by_link = reply_html("Fixed.", "TKT-1", Some("https://help.test/x"), &["fix.pdf".into()], false);
        assert!(by_link.contains("open the ticket to download them"), "{by_link}");
        let bare = reply_html("Fixed.", "TKT-1", None, &[], false);
        assert!(!bare.contains("href") && bare.contains("Ticket TKT-1"), "{bare}");
    }

    #[test]
    fn files_are_named_by_id_once() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        assert_eq!(file_ids(Some(&json!([{"id": a.to_string(), "file_name": "x"}, b.to_string(), {"id": a.to_string()}, {"id": "nonsense"}]))), vec![a, b]);
        assert!(file_ids(None).is_empty() && file_ids(Some(&json!("x"))).is_empty());
    }
}
