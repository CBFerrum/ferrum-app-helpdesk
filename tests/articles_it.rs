//! Article translations and published versions (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): what is published is
//! kept as a version each time, numbered upwards, the latest 50; a version can be restored; a translation is an article
//! linked to its original, one per language, one level; readers are offered one article per family in their language.
//! One scenario: the suites share one database and reset it.

mod common;
use common::*;
use ferrum_kernel::capability::Invocation;
use ferrum_kernel::{Error, Result};
use serde_json::{Value, json};
use uuid::Uuid;

#[test]
fn translations_and_versions() {
    run(scenario());
}

fn on<T: std::fmt::Debug>(r: &Result<T>, field: &str) -> bool {
    matches!(r, Err(Error::Validation(f)) if f.iter().any(|x| x.field == field))
}

/// The article's version numbers, in order (the lines' stored order is not what this checks).
fn numbers(a: &ferrum_kernel::Doc) -> Vec<i64> {
    let mut n: Vec<i64> = a["versions"].as_array().unwrap_or_else(|| panic!("{a:?}")).iter().map(|v| v["version_no"].as_i64().unwrap()).collect();
    n.sort();
    n
}

fn titles(r: &Value) -> Vec<String> {
    r["articles"].as_array().unwrap_or_else(|| panic!("{r}")).iter().map(|a| a["title"].as_str().unwrap().to_string()).collect()
}

async fn scenario() {
    let e = boot("articles", false, vec![]).await;
    let k = e.k.clone();
    let category = uid(&e.list("KbCategory", json!({"category_name": "General"})).await[0]);
    let new = |title: &'static str, extra: Value| {
        let e = &e;
        async move {
            let mut v = json!({"title": title, "category": category.to_string(), "visibility": "Customers Only", "status": "Draft", "content": format!("{title} text")});
            v.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
            e.k.insert(&e.admin, "KbArticle", obj(v)).await
        }
    };

    // ── publishing_keeps_a_version ──
    let a = uid(&new("Reset your password", json!({"summary": "How"})).await.unwrap());
    assert!(numbers(&e.get("KbArticle", a).await).is_empty(), "a draft has none");
    let p = e.update("KbArticle", a, json!({"status": "Published"})).await;
    let v1 = &p["versions"][0];
    assert_eq!((v1["version_no"].clone(), v1["title"].clone(), v1["summary"].clone()), (json!(1), json!("Reset your password"), json!("How")));
    assert_eq!((v1["content"].clone(), v1["published_by"].clone()), (json!("Reset your password text"), json!("admin@articles.test")));
    assert!(v1["published_at"].is_string());

    // ── a_change_while_published_keeps_another_and_a_draft_edit_none ──
    assert_eq!(numbers(&e.update("KbArticle", a, json!({"content": "Open Settings, then Security."})).await), [1, 2]);
    let other = uid(&e
        .insert(
            "KbCategory",
            json!({"category_name": "Accounts", "parent_category": e.list("KbCategory", json!({"category_name": "All Categories"})).await[0]["id"]}),
        )
        .await);
    assert_eq!(numbers(&e.update("KbArticle", a, json!({"category": other.to_string()})).await), [1, 2], "a category is not what customers read");
    e.update("KbArticle", a, json!({"status": "Archived"})).await;
    assert_eq!(numbers(&e.update("KbArticle", a, json!({"content": "Being rewritten"})).await), [1, 2], "not published: no version");
    assert_eq!(numbers(&e.update("KbArticle", a, json!({"status": "Published"})).await), [1, 2, 3]);

    // ── versions_count_up ──
    let many = uid(&new("Many", json!({"status": "Published"})).await.unwrap());
    for i in 0..51 {
        e.update("KbArticle", many, json!({"content": format!("Edit {i}")})).await;
    }
    let kept = numbers(&e.get("KbArticle", many).await);
    assert_eq!(kept, (3..=52).collect::<Vec<i64>>(), "the latest 50, numbered on");

    // ── a_client_cannot_write_versions ──
    let before = e.get("KbArticle", a).await["versions"].clone();
    e.update("KbArticle", a, json!({"versions": []})).await;
    e.update("KbArticle", a, json!({"versions": [{"version_no": 99, "title": "Forged", "content": "x"}]})).await;
    assert_eq!(e.get("KbArticle", a).await["versions"], before);

    // ── restore_brings_a_version_back ──
    let r = e.action(&e.admin, "KbArticle", a, "restore_version", json!({"version": 1})).await.expect("the restore");
    assert_eq!(r, json!({"restored": 1, "version_no": 4}));
    let now = e.get("KbArticle", a).await;
    assert_eq!(now["content"], json!("Reset your password text"));
    assert_eq!(numbers(&now), [1, 2, 3, 4], "nothing removed");

    // ── restore_is_refused_without_write_and_for_an_unknown_version ──
    let sam = e.user("sam@acme.test", &["Support Agent"]).await; // writes articles
    let reader: ferrum_meta::PermissionDef = serde_json::from_value(json!({"entity": "KbArticle", "read": true})).unwrap();
    k.set_role_permission(&e.admin, "Article Reader", &reader).await.unwrap();
    let ray = e.user("ray@acme.test", &["Article Reader"]).await;
    assert!(e.action(&ray, "KbArticle", a, "restore_version", json!({"version": 1})).await.is_err(), "reads only");
    assert!(on(&e.action(&sam, "KbArticle", a, "restore_version", json!({"version": 99})).await, "version"));

    // ── restore_repeats_under_one_key ──
    let again = || Invocation::new("helpdesk.kb_article.restore_version", json!({"id": a, "params": {"version": 2}})).key("restore-2");
    let first = k.invoke(&sam, again()).await.unwrap();
    let second = k.invoke(&sam, again()).await.unwrap();
    assert!(!first.repeated && second.repeated);
    assert_eq!(numbers(&e.get("KbArticle", a).await), [1, 2, 3, 4, 5], "one new version");

    // ── one_translation_per_language ──
    let hi = uid(&new("पासवर्ड रीसेट करें", json!({"translation_of": a.to_string(), "language": " HI ", "status": "Published", "content": "password पासवर्ड"}))
        .await
        .unwrap());
    assert_eq!(e.get("KbArticle", hi).await["language"], json!("hi"), "kept trimmed and lower case");
    assert!(new("दूसरा", json!({"translation_of": a.to_string(), "language": "hi"})).await.is_err(), "one per language");
    let b = uid(&new("Warranty terms", json!({"status": "Published"})).await.unwrap());
    new("वारंटी", json!({"translation_of": b.to_string(), "language": "hi"})).await.expect("another original's");
    assert!(on(&new("No language", json!({"translation_of": b.to_string()})).await, "language"));

    // ── translations_are_one_level ──
    assert!(on(&new("Of a translation", json!({"translation_of": hi.to_string(), "language": "fr"})).await, "translation_of"));
    assert!(on(&k.update(&e.admin, "KbArticle", a, obj(json!({"translation_of": b.to_string(), "language": "de"})), None).await, "translation_of"));
    assert!(k.update(&e.admin, "KbArticle", b, obj(json!({"translation_of": b.to_string(), "language": "de"})), None).await.is_err(), "its own");

    // ── a_reader_is_offered_their_language ──
    let pia = e.portal_user("pia@garage.test").await;
    let get = |path: String| {
        let (k, pia, tenant) = (k.clone(), pia.clone(), e.tenant);
        async move { call(&k, Some(&pia), tenant, "GET", &path, Value::Null).await.unwrap() }
    };
    let words = "password";
    let hindi = titles(&get(format!("portal/suggest?subject={words}&lang=hi")).await);
    assert!(hindi.contains(&"पासवर्ड रीसेट करें".to_string()) && !hindi.contains(&"Reset your password".to_string()), "{hindi:?}");
    let variant = titles(&get(format!("portal/suggest?subject={words}&lang=hi-IN")).await);
    assert!(variant.contains(&"पासवर्ड रीसेट करें".to_string()), "{variant:?}");
    for path in [format!("portal/suggest?subject={words}&lang=fr"), format!("portal/suggest?subject={words}")] {
        let got = titles(&get(path).await);
        assert!(got.contains(&"Reset your password".to_string()) && !got.contains(&"पासवर्ड रीसेट करें".to_string()), "{got:?}");
    }

    // ── the_article_page_names_its_other_languages ──
    let page = get(format!("portal/articles/{a}")).await;
    assert_eq!(page["translations"], json!([{"id": hi, "language": "hi", "title": "पासवर्ड रीसेट करें"}]));

    // ── a_customer_never_sees_an_unpublished_translation ──
    let de = uid(&new("Passwort zurücksetzen", json!({"translation_of": a.to_string(), "language": "de"})).await.unwrap());
    let internal =
        uid(&new("Réinitialiser", json!({"translation_of": a.to_string(), "language": "fr", "status": "Published", "visibility": "Internal"})).await.unwrap());
    let page = get(format!("portal/articles/{a}")).await;
    let ids: Vec<Value> = page["translations"].as_array().unwrap().iter().map(|t| t["id"].clone()).collect();
    assert!(!ids.contains(&json!(de)) && !ids.contains(&json!(internal)), "{page}");
    for lang in ["de", "fr"] {
        let got = titles(&get(format!("portal/articles?q=password&lang={lang}")).await);
        assert!(!got.iter().any(|t| t == "Passwort zurücksetzen" || t == "Réinitialiser"), "{got:?}");
    }

    // ── an_agent_is_offered_the_customers_language ──
    let ticket = uid(&e.insert("Ticket", json!({"subject": "Password reset", "language": "hi", "contact_email": "pia@garage.test"})).await);
    let r = titles(&e.action(&sam, "Ticket", ticket, "suggest_articles", json!({"q": "password"})).await.unwrap());
    assert!(r.contains(&"पासवर्ड रीसेट करें".to_string()) && !r.contains(&"Reset your password".to_string()), "{r:?}");
    let _: Uuid = a;
}
