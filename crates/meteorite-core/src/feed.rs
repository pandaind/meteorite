//! Generated machine-readable files: feeds, sitemap, robots.txt, search index.

use crate::config::Config;
use crate::site::{Page, Site};
use crate::util::{strip_anchors, strip_tags, xml_escape};

/// Dated regular pages, newest first.
fn recent(site: &Site, limit: usize) -> Vec<&Page> {
    let mut pages: Vec<&Page> = site
        .all
        .iter()
        .filter(|p| !p.is_index && p.date_parsed.is_some())
        .collect();
    pages.sort_by(|a, b| {
        b.date_parsed
            .cmp(&a.date_parsed)
            .then_with(|| a.url.cmp(&b.url))
    });
    pages.truncate(limit);
    pages
}

pub fn atom(config: &Config, site: &Site) -> String {
    let pages = recent(site, config.feed.limit);
    let base = config.base_url.trim_end_matches('/');
    let updated = pages
        .first()
        .and_then(|p| p.date.clone())
        .unwrap_or_else(|| "1970-01-01".to_string());
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    s.push_str(&format!("  <title>{}</title>\n", xml_escape(&config.title)));
    s.push_str(&format!(
        "  <link href=\"{base}/atom.xml\" rel=\"self\"/>\n"
    ));
    s.push_str(&format!("  <link href=\"{base}/\"/>\n"));
    s.push_str(&format!("  <id>{base}/</id>\n"));
    s.push_str(&format!("  <updated>{updated}T00:00:00Z</updated>\n"));
    for p in pages {
        s.push_str("  <entry>\n");
        s.push_str(&format!(
            "    <title>{}</title>\n",
            xml_escape(&p.meta.title)
        ));
        s.push_str(&format!(
            "    <link href=\"{}\"/>\n",
            xml_escape(&p.permalink)
        ));
        s.push_str(&format!("    <id>{}</id>\n", xml_escape(&p.permalink)));
        s.push_str(&format!(
            "    <updated>{}T00:00:00Z</updated>\n",
            p.date.as_deref().unwrap_or("1970-01-01")
        ));
        let summary = match &p.meta.description {
            Some(d) if !d.is_empty() => d.clone(),
            _ => strip_tags(&p.summary),
        };
        s.push_str(&format!(
            "    <summary>{}</summary>\n",
            xml_escape(&summary)
        ));
        s.push_str("  </entry>\n");
    }
    s.push_str("</feed>\n");
    s
}

pub fn rss(config: &Config, site: &Site) -> String {
    let pages = recent(site, config.feed.limit);
    let base = config.base_url.trim_end_matches('/');
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str("<rss version=\"2.0\">\n<channel>\n");
    s.push_str(&format!("  <title>{}</title>\n", xml_escape(&config.title)));
    s.push_str(&format!("  <link>{base}/</link>\n"));
    s.push_str(&format!(
        "  <description>{}</description>\n",
        xml_escape(&config.title)
    ));
    s.push_str(&format!(
        "  <language>{}</language>\n",
        xml_escape(&config.language)
    ));
    for p in pages {
        s.push_str("  <item>\n");
        s.push_str(&format!(
            "    <title>{}</title>\n",
            xml_escape(&p.meta.title)
        ));
        s.push_str(&format!("    <link>{}</link>\n", xml_escape(&p.permalink)));
        s.push_str(&format!("    <guid>{}</guid>\n", xml_escape(&p.permalink)));
        if let Some(d) = p.date_parsed {
            s.push_str(&format!(
                "    <pubDate>{} 00:00:00 +0000</pubDate>\n",
                d.format("%a, %d %b %Y")
            ));
        }
        let summary = match &p.meta.description {
            Some(d) if !d.is_empty() => d.clone(),
            _ => strip_tags(&p.summary),
        };
        s.push_str(&format!(
            "    <description>{}</description>\n",
            xml_escape(&summary)
        ));
        s.push_str("  </item>\n");
    }
    s.push_str("</channel>\n</rss>\n");
    s
}

pub fn sitemap(site: &Site) -> String {
    let mut urls: Vec<(&str, Option<&str>)> = site
        .all
        .iter()
        .map(|p| (p.permalink.as_str(), p.date.as_deref()))
        .collect();
    urls.sort();
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (loc, lastmod) in urls {
        s.push_str("  <url>");
        s.push_str(&format!("<loc>{}</loc>", xml_escape(loc)));
        if let Some(d) = lastmod {
            s.push_str(&format!("<lastmod>{d}</lastmod>"));
        }
        s.push_str("</url>\n");
    }
    s.push_str("</urlset>\n");
    s
}

pub fn robots(config: &Config, with_sitemap: bool) -> String {
    let mut s = String::from("User-agent: *\nAllow: /\n");
    if with_sitemap {
        s.push_str(&format!(
            "\nSitemap: {}/sitemap.xml\n",
            config.base_url.trim_end_matches('/')
        ));
    }
    s
}

/// A flat JSON array for client-side search: `[{title,url,description,tags,text}]`.
pub fn search_index(config: &Config, site: &Site) -> String {
    let mut entries = Vec::new();
    for p in &site.all {
        let mut text = strip_tags(&strip_anchors(&p.content));
        if text.chars().count() > 4000 {
            text = text.chars().take(4000).collect();
        }
        let entry = SearchEntry {
            title: &p.meta.title,
            url: &p.url,
            description: p.meta.description.as_deref().unwrap_or(""),
            tags: &p.meta.tags,
            text,
        };
        entries.push(entry);
    }
    let _ = config;
    tera::to_value(&entries)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "[]".into())
}

#[derive(serde::Serialize)]
struct SearchEntry<'a> {
    title: &'a str,
    url: &'a str,
    description: &'a str,
    tags: &'a [String],
    text: String,
}
