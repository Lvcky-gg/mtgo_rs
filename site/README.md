# MTGO RS web hub

The entry point is `index.html` at the repository root. `site/styles.css` and
`site/hub.js` supply the responsive layout, resource filters and release lookup.
There is no build step or package install. Resource links are ordinary HTML and
remain available without JavaScript. Fonts have local system fallbacks.

## Preview

From the repository root, run `python3 -m http.server 8080`, then visit
`http://localhost:8080`. Stop the preview with Ctrl+C.

## GitHub Pages

In the repository's **Settings → Pages → Build and deployment**, choose
**GitHub Actions** as the source. The `GitHub Pages` workflow deploys after
website changes are pushed to `main`, or via its manual workflow dispatch.
The default URL is `https://lvcky-gg.github.io/mtgo_rs/`.

The workflow stages only `index.html` and `site/`, rather than publishing the
repository's application data or internal evidence. Relative asset URLs work
at the project subpath and on a custom domain. No deployment was performed by
creating these files.

## Downloads and resource maintenance

The browser requests GitHub's latest stable release API and matches the four
native package suffixes used by the existing release workflow. Direct downloads
use the returned GitHub asset URLs; missing packages point to the release page.
If GitHub is unavailable or rate-limited, static latest-release links remain.
Checksum and release-note links update alongside the packages. macOS architecture
is explicitly chosen by the visitor; it is not guessed from the browser.

Edit resource cards directly in `index.html`. Categories are the card's
`data-category` value. Update the displayed resource count when adding links.
Rules and ban-list links point to official landing pages so future updates do not
require embedding a dated document or ban list in this site.

The hub makes no analytics requests. External requests are limited to Google
Fonts and GitHub's public release API until a visitor follows an external link.
