"""Assemble the browser UI as one self-contained HTML document.

The UI ships as web/index.html plus /assets/app.css and /assets/app.js.
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
    html = html.replace(CSS_LINK, "<style>\n" + read("css", "app.css") + "</style>", 1)
    return html.replace(JS_TAG, "<script>\n" + read("js", "app.js") + "</script>", 1)
