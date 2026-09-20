# Fella documentation hosting

This is the deployment reference for the public Fella documentation. The
marketing site and the product documentation have separate source code and
separate hostnames: `lilfella.app` for the product, `docs.lilfella.app` for
the guide.

## End state

| Responsibility | Canonical location | Owner |
| --- | --- | --- |
| Marketing, install scripts, and project thesis | `https://lilfella.app` | `fella-web` Worker |
| Product and contributor documentation | `https://docs.lilfella.app` | Mintlify, sourced from `fella` |
| Legacy static guide | Removed | `fella-web/marketing/docs.html` is deleted |
| Documentation source | Root MDX files and `docs.json` | `fella` `main` branch |

This is a subdomain, not a subpath. Mintlify owns `docs.lilfella.app`
directly via its own custom-domain flow (standard TXT + CNAME verification,
Mintlify manages TLS). No Cloudflare Worker sits in front of it, no path
rewriting, no base-path configuration. `fella-web` only needs to redirect the
old `/docs` subpath (and the even older `/docs.html`) to the new hostname for
existing bookmarks and links.

We tried the subpath approach first (`lilfella.app/docs` behind a
hand-rolled `fella-web` Worker proxy) and reverted it: it required getting a
Worker's path-stripping logic, Mintlify's "Host at" base-path verification,
and Cloudflare cache-bypass rules all correct simultaneously, and even then
hit an unresolved intermittent-cache issue. A subdomain is Mintlify's
best-supported path and what most Mintlify-hosted docs sites (including
Anthropic's) actually use.

## Mintlify project setup

1. Sign in to Mintlify and install the GitHub App for the
   `Avijit-Kumar-GIT/fella` repository.
2. Create or select the Fella documentation project and set its production
   branch to `main`.
3. Set the documentation root to the repository root. The project must see
   `docs.json`, `index.mdx`, the page directories, `logo.svg`, and
   `favicon.svg`.
4. Keep the repository's `.mintignore` active. It excludes application source,
   fixtures, benchmarks, and internal engineering notes from the published
   site.
5. Use the Mintlify preview or a branch deployment to review documentation
   changes before merging them into `main`.

Mintlify deploys changes from the connected branch. No generated documentation
output is committed to this repository.

## Custom domain setup

In the Mintlify dashboard, open **Settings → Custom domain** (or wherever the
dashboard currently labels domain setup) and add:

```text
Domain: docs.lilfella.app
```

Do **not** enable a base path / "Host at" option — this is a plain custom
domain on its own subdomain, not a subpath deployment.

Mintlify will display TXT verification records specific to this domain
(these are different from any records generated for a `lilfella.app`
apex-based setup — do not reuse old ones). Add them to Cloudflare DNS exactly
as shown, as **DNS only** (grey cloud, not proxied):

```text
TXT   _acme-challenge.docs.lilfella.app       <value from Mintlify>
TXT   _cf-custom-hostname.docs.lilfella.app   <value from Mintlify>
```

Wait for both to validate (Mintlify's dashboard shows a green check per
record; typically minutes, use "Re-verify domain" if it stalls). Once
validated, add the CNAME Mintlify gives you:

```text
CNAME   docs.lilfella.app   cname.mintlify.builders
```

Keep this **DNS only** as well — Mintlify terminates TLS for this hostname
itself; Cloudflare proxying it isn't part of the supported flow.

The current Mintlify docs are the authority for exact dashboard labels:
[Mintlify: custom domain](https://www.mintlify.com/docs/customize/custom-domain).

## fella-web changes

`fella-web`'s `worker.js` no longer proxies anything — it just serves the
static marketing site. `marketing/_redirects` sends the old subpath and
legacy static guide to the new hostname:

```text
/docs.html    https://docs.lilfella.app             301
/docs         https://docs.lilfella.app             301
/docs/*       https://docs.lilfella.app/:splat      301
```

Nothing in `fella-web` needs a Cloudflare Worker variable, a custom Worker
route for `/docs`, or a cache-bypass rule for it — those were all
subpath-specific and have been removed.

## Verification checklist

```sh
curl -I https://lilfella.app/
curl -I https://lilfella.app/docs
curl -I https://docs.lilfella.app/
curl -fsSL https://docs.lilfella.app/llms.txt >/dev/null
```

Expected behavior:

- The root and marketing routes still return the `fella-web` site.
- `/docs` on the apex domain 301s to `https://docs.lilfella.app`.
- `docs.lilfella.app` loads Mintlify directly.
- `llms.txt` and the rest of Mintlify's site work normally on the subdomain.
- No published navigation link points to the retired static guide or the old
  `/docs` subpath.

If `docs.lilfella.app` doesn't resolve, check that both TXT records are
green in Mintlify's dashboard and the CNAME has been added. If it resolves
but shows a certificate error, the domain likely isn't fully verified yet —
wait for Mintlify to finish provisioning.
