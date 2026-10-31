"""Assemble the browser UI as one self-contained HTML document.

The UI ships as web/index.html plus /assets/app.css and /assets/app.js
(both assembled from the ordered fragments in web/css/app/ and web/js/app/).
Verification and screenshot harnesses serve a single page, so this inlines
the first-party assets exactly where the browser would load them.
"""
import os

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CSS_LINK = '<link rel="stylesheet" href="/assets/app.css">'
JS_TAG = '<script src="/assets/app.js"></script>'


def load_index_html(repo=REPO):
    def read(*parts):
        with open(os.path.join(repo, "web", *parts), encoding="utf-8") as fh:
            return fh.read()

    html = read("index.html")
    for marker in (CSS_LINK, JS_TAG):
        if marker not in html:
            raise RuntimeError(f"asset marker not found in web/index.html: {marker}")
    css_dir = os.path.join(repo, "web", "css", "app")
    css = "".join(read("css", "app", name) for name in sorted(os.listdir(css_dir)) if name.endswith(".css"))
    html = html.replace(CSS_LINK, "<style>\n" + css + "</style>", 1)
    # Same ordering as src/web_ui.rs APP_JS_PARTS: fragments sorted by name.
    parts_dir = os.path.join(repo, "web", "js", "app")
    script = "".join(read("js", "app", name) for name in sorted(os.listdir(parts_dir)) if name.endswith(".js"))
    return html.replace(JS_TAG, "<script>\n" + script + "</script>", 1)
