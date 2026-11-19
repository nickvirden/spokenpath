# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's private vulnerability reporting:
[open a private security advisory](https://github.com/nickvirden/spokenpath/security/advisories/new).
If that link does not open the advisory form, contact the maintainer through their
[GitHub profile](https://github.com/nickvirden) and ask for a private channel, without describing
the vulnerability in public.

Never report a vulnerability in a public issue, pull request or discussion. A public report
discloses the problem before a fix exists.

Include what you found, how to reproduce it (a minimal page or command helps most), which version
or commit you tested, and what an attacker could do with it.

## Supported versions

spokenpath is pre-alpha. Only the `main` branch is supported, and fixes land there. There are no
maintained release branches.

## What to expect

spokenpath has one maintainer. Reports are acknowledged and assessed as soon as the maintainer is
able, and you are kept informed through the advisory while a fix is prepared. There is no
guaranteed response time. When a fix is released, the advisory is published with credit to the
reporter unless you ask otherwise.

## Trust model

Capturing a page means loading it in a real browser, which runs the page's scripts. Capture only
pages you trust, such as your own project's pages and fixtures, as you would before opening them
in your own browser.

Capture is designed to serve the page locally and block external network requests by default.
That is a design intent, not a sandbox: it limits accidental network access, but it does not
isolate a hostile page from the browser or the machine running it.

Chromium capture answers every request itself, by request interception, and opens no listening
socket. Safari capture (`--cross-check safari`) cannot intercept requests, so it serves the page
over HTTP on a loopback port (`127.0.0.1`) for the length of the capture. It refuses requests that
name another host, sends `Cross-Origin-Resource-Policy: same-origin` so a page on another origin
cannot embed what it serves, and sends a Content-Security-Policy that limits the page's
sub-resource loads and form submissions to that origin. The policy does not stop top-level
navigation (a script setting `location`, a meta refresh), resource hints or WebRTC. Capture detects
a page that has navigated away from the served origin and fails with `page-left-origin`, but it
does not prevent the request the page made. This, too, is a design intent and not a sandbox: other
programs on the same machine can reach the loopback port while the capture runs, and the page's
scripts run in the Mac's real Safari.
