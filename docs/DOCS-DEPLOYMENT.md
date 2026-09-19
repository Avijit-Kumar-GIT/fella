# Fella documentation hosting

This is the deployment reference for the public Fella documentation. The
marketing site and the product documentation have separate source code, while
people see one site: `lilfella.app` for the product and `lilfella.app/docs`
for the guide.

## End state

| Responsibility | Canonical location | Owner |
| --- | --- | --- |
| Marketing, install scripts, and project thesis | `https://lilfella.app` | `fella-web` Worker |
| Product and contributor documentation | `https://lilfella.app/docs` | Mintlify, sourced from `fella` |
| Legacy static guide | Removed | `fella-web/marketing/docs.html` is deleted |
| Documentation source | Root MDX files and `docs.json` | `fella` `main` branch |

The `/docs` URL is a path on the existing marketing domain. A DNS record
cannot route only one path, so the `fella-web` Cloudflare Worker proxies the
documentation request to Mintlify and sends every other request to the static
marketing assets. This keeps the public URL simple without maintaining a
second copy of the guide.

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

## Configure the `/docs` subpath in Mintlify

In the Mintlify dashboard, open **Settings → Custom domain** and configure the
custom domain as:

```text
Domain:   lilfella.app
Base path: /docs
```

If the dashboard labels this option **Host at**, enable it and enter `/docs`.
Mintlify will show the project-specific upstream hostname and, for some
accounts, a verification record. The upstream hostname is unique to the
Mintlify project; do not guess it or replace it with `lilfella.app`.

Copy the hostname Mintlify gives you into the production `fella-web` Worker as
the non-secret variable `MINTLIFY_HOST`. It must be the hostname only, without
`https://` or a path. The Worker uses it for the upstream `Host` header while
preserving the public `lilfella.app/docs` URL.

The current Mintlify Cloudflare guide is the authority for the exact dashboard
labels and generated upstream value:

- [Mintlify: Cloudflare deployment and subpath proxy](https://www.mintlify.com/docs/deploy/cloudflare)
- [Mintlify: host documentation at a subpath](https://www.mintlify.com/docs/deploy/docs-subpath)

## DNS records

There is no `docs.lilfella.app` DNS record in this design. The existing
Cloudflare DNS/custom-domain route for `lilfella.app` stays attached to the
`fella-web` Worker, which is what makes path routing possible.

Add a DNS record only if Mintlify displays one for domain verification. Copy
the record name, type, and value exactly from the Mintlify dashboard; those
values are deployment-specific and must not be hardcoded in this repository.
Do not replace the apex `lilfella.app` record with a CNAME to Mintlify, because
that would take the marketing site away from the Worker. Do not create a
`docs` subdomain record for this `/docs` deployment.

If the Mintlify dashboard gives a TXT or CNAME verification record, add it in
Cloudflare DNS and wait for Mintlify to verify it. Keep the existing apex and
`www` records unchanged.

## Cloudflare Worker deployment

The Worker implementation lives in the `fella-web` repository:

- `worker.js` proxies `/docs`, `/docs/*`, `/mintlify-assets/*`, and
  `/_mintlify/*` to Mintlify.
- `worker.js` passes `/.well-known/*` to the static asset service so domain
  verification files remain reachable.
- All other paths are served from `fella-web/marketing`.
- `marketing/_redirects` keeps only the old `/docs.html → /docs` compatibility
  redirect; it must not redirect `/docs` to another hostname.

In the Cloudflare Worker dashboard, add:

```text
Variable name:  MINTLIFY_HOST
Value:          <the exact upstream hostname from Mintlify>
Type:           Plain text / non-secret
```

Then deploy the `fella-web` Worker. The deploy command is:

```sh
npx wrangler deploy
```

The Worker deliberately returns a clear `503` for documentation requests if
`MINTLIFY_HOST` is missing. That makes an incomplete setup visible instead of
silently serving a marketing 404 page.

## Verification checklist

After Mintlify has verified the domain and the Worker variable is set:

```sh
curl -I https://lilfella.app/
curl -I https://lilfella.app/docs
curl -I https://lilfella.app/docs/
curl -fsSL https://lilfella.app/docs/llms.txt >/dev/null
curl -I https://lilfella.app/docs.html
```

Expected behavior:

- The root and marketing routes still return the `fella-web` site.
- `/docs` loads Mintlify at the same public hostname and does not redirect to
  a docs subdomain.
- Mintlify assets and `llms.txt` work below the same `/docs` origin.
- `/docs.html` permanently redirects to `/docs` for old bookmarks.
- No published navigation link points to the retired static guide or the old
  `docs.lilfella.app` hostname.

If `/docs` returns `503`, set `MINTLIFY_HOST` from the Mintlify dashboard. If
it returns the marketing 404, check that the `fella-web` Worker is deployed
with `ASSETS` bound and that its custom domain still includes
`lilfella.app`. If Mintlify reports a domain error, check the exact
verification record it supplied and leave the apex record pointed at
Cloudflare.

## Rollback

To temporarily disable the proxy, deploy the previous `fella-web` Worker
commit or remove the `/docs` proxy branch from `worker.js`. Keep Mintlify's
project and the source documentation intact so the route can be restored
without recreating the docs site.
