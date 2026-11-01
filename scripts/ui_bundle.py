"""Assemble the browser UI for the verification and screenshot harnesses.

The UI ships as web/index.html plus /assets/app.css (the ordered fragments in
web/css/app/) and the native ES modules in web/js/app/, which the server
embeds and serves unchanged at /assets/js/<name>; index.html loads
/assets/js/main.js as a module script and the browser follows its imports.
load_index_html() inlines the stylesheet exactly where the browser would load
it and keeps the module script tag; a harness serves module_assets() (or
answers module_source()) alongside the page, just like the real server.
"""
import os
import urllib.parse

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CSS_LINK = '<link rel="stylesheet" href="/assets/app.css">'
JS_TAG = '<script src="/assets/js/main.js" type="module"></script>'
MODULE_PREFIX = "/assets/js/"
MODULE_TYPE = "text/javascript; charset=utf-8"


def _read(repo, *parts):
    with open(os.path.join(repo, "web", *parts), encoding="utf-8") as fh:
        return fh.read()


def load_index_html(repo=REPO):
    html = _read(repo, "index.html")
    for marker in (CSS_LINK, JS_TAG):
        if marker not in html:
            raise RuntimeError(f"asset marker not found in web/index.html: {marker}")
    css_dir = os.path.join(repo, "web", "css", "app")
    css = "".join(_read(repo, "css", "app", name) for name in sorted(os.listdir(css_dir)) if name.endswith(".css"))
    return html.replace(CSS_LINK, "<style>\n" + css + "</style>", 1)


def module_assets(repo=REPO):
    """{"/assets/js/<name>": bytes} for every app module, as the server embeds them."""
    module_dir = os.path.join(repo, "web", "js", "app")
    assets = {}
    for name in sorted(os.listdir(module_dir)):
        if name.endswith(".js"):
            with open(os.path.join(module_dir, name), "rb") as fh:
                assets[MODULE_PREFIX + name] = fh.read()
    return assets


def module_source(url, repo=REPO):
    """Body of the app module a request URL names, or None."""
    return module_assets(repo).get(urllib.parse.urlsplit(url).path)
