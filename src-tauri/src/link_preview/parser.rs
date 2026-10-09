use super::{safety::safe_url, LinkPreview, LinkPreviewStatus};
use reqwest::Url;
use scraper::{Html, Selector};

pub fn parse(html: &str, base: &Url, preview: &mut LinkPreview) {
    let document = Html::parse_document(html);
    let meta = Selector::parse("meta").unwrap();
    let value = |key: &str| {
        document.select(&meta).find_map(|element| {
            let attr = element.value();
            let name = attr.attr("property").or_else(|| attr.attr("name"))?;
            if !name.eq_ignore_ascii_case(key) {
                return None;
            }
            clean(attr.attr("content")?)
        })
    };
    preview.title = value("og:title")
        .or_else(|| value("twitter:title"))
        .or_else(|| {
            document
                .select(&Selector::parse("title").unwrap())
                .find_map(|element| clean(&element.text().collect::<String>()))
        });
    preview.description = value("og:description")
        .or_else(|| value("twitter:description"))
        .or_else(|| value("description"));
    let resolve = |value: &str| {
        base.join(value)
            .ok()
            .and_then(|url| safe_url(url.as_str()))
            .map(|url| url.to_string())
    };
    preview.image_url = value("og:image")
        .and_then(|v| resolve(&v))
        .or_else(|| value("twitter:image").and_then(|v| resolve(&v)));
    preview.favicon_url = document
        .select(&Selector::parse("link[rel][href]").unwrap())
        .find_map(|element| {
            let attr = element.value();
            if !attr
                .attr("rel")?
                .split_ascii_whitespace()
                .any(|rel| rel.eq_ignore_ascii_case("icon"))
            {
                return None;
            }
            resolve(attr.attr("href")?)
        })
        .or_else(|| resolve("/favicon.ico"));
    preview.status = match (&preview.title, &preview.description, &preview.image_url) {
        (Some(_), Some(_), Some(_)) => LinkPreviewStatus::Success,
        (None, None, None) => LinkPreviewStatus::Unavailable,
        _ => LinkPreviewStatus::Partial,
    };
    // ponytail: recognize common login redirect paths; extend for confirmed misses.
    if base.as_str() != preview.url
        && base.path().split('/').any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "login" | "signin" | "sign-in" | "authorize"
            )
        })
    {
        preview.title = None;
        preview.description = None;
        preview.image_url = None;
        preview.status = LinkPreviewStatus::Unavailable;
    }
}

fn clean(value: &str) -> Option<String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        None
    } else {
        Some(value.chars().take(2000).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_precedence_fallback_and_relative_urls() {
        let base = Url::parse("https://example.com/path/page").unwrap();
        let mut p = LinkPreview::unavailable(base.as_str());
        parse(
            r#"<title>Fallback</title><meta name='twitter:title' content='Twitter'><meta property='og:title' content='OG &amp; title'><meta property='og:description' content='Description'><meta property='og:image' content='../card.png'><link rel='shortcut icon' href='/icon.png'>"#,
            &base,
            &mut p,
        );
        assert_eq!(p.title.as_deref(), Some("OG & title"));
        assert_eq!(p.description.as_deref(), Some("Description"));
        assert_eq!(p.image_url.as_deref(), Some("https://example.com/card.png"));
        assert_eq!(
            p.favicon_url.as_deref(),
            Some("https://example.com/icon.png")
        );
        assert_eq!(p.status, LinkPreviewStatus::Success);
        parse(
            "<title>Fallback</title><meta name=description content='Text'>",
            &base,
            &mut p,
        );
        assert_eq!(p.title.as_deref(), Some("Fallback"));
        assert_eq!(p.description.as_deref(), Some("Text"));
        assert_eq!(p.status, LinkPreviewStatus::Partial);
        parse(
            "<meta property='og:image' content='http://127.0.0.1/image'>",
            &base,
            &mut p,
        );
        assert!(p.image_url.is_none());
        assert_eq!(p.status, LinkPreviewStatus::Unavailable);
        parse("<meta name='twitter:title' content='Twitter'><meta name='twitter:description' content='Tweet'><meta name='twitter:image' content='/twitter.png'>", &base, &mut p);
        assert_eq!(p.title.as_deref(), Some("Twitter"));
        assert_eq!(p.description.as_deref(), Some("Tweet"));
        assert_eq!(
            p.image_url.as_deref(),
            Some("https://example.com/twitter.png")
        );
        let login = Url::parse("https://example.com/login").unwrap();
        parse("<title>Sign in</title>", &login, &mut p);
        assert_eq!(p.status, LinkPreviewStatus::Unavailable);
        assert!(p.title.is_none());
    }
}
