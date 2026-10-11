import DOMPurify from "dompurify";
import { Marked } from "marked";

const markdown = new Marked({
  gfm: true,
  renderer: {
    // Release notes are Markdown; embedded HTML and remote media are unnecessary.
    html: () => "",
    image: () => "",
  },
});

export function externalReleaseNotesUrl(value: string): string | null {
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
  } catch {
    return null;
  }
}

export function renderReleaseNotes(content: string): string {
  const html = markdown.parse(content, { async: false });
  const sanitized = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: [
      "h1", "h2", "h3", "h4", "h5", "h6", "p", "br", "hr",
      "strong", "em", "del", "a", "ul", "ol", "li", "blockquote",
      "pre", "code", "table", "thead", "tbody", "tr", "th", "td", "input",
    ],
    ALLOWED_ATTR: ["href", "title", "start", "type", "disabled", "checked"],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  });
  const template = document.createElement("template");
  template.innerHTML = sanitized;

  for (const link of template.content.querySelectorAll("a")) {
    const href = externalReleaseNotesUrl(link.getAttribute("href") ?? "");
    if (href) {
      link.href = href;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
    } else {
      link.removeAttribute("href");
    }
  }

  // GFM task lists are informational, not editable form controls.
  for (const input of template.content.querySelectorAll("input")) {
    input.type = "checkbox";
    input.disabled = true;
  }

  return template.innerHTML;
}
