# Fella documentation hosting

This is the deployment reference for the public Fella documentation. It keeps
the marketing site and the product documentation separate without maintaining
two copies of the user guide.

## End state

| Responsibility | Canonical location |
| --- | --- |
| Marketing, install scripts, project thesis | `https://lilfella.app` from `fella-web` |
| Product and contributor documentation | `https://docs.lilfella.app` from this repository through Mintlify |
| Existing marketing-site `/docs` links | Permanent redirect to `https://docs.lilfella.app` |
| Source of documentation content | Root MDX files and `docs.json` in `fella` |

The static `fella-web/marketing/docs.html` page is retired. It was a separate
copy of the guide and had drifted from the lean release: it still described
local Ollama, Packs, connectors, and the old Python boundary. The Mintlify
pages in this repository describe the shipped BYOK personal release and are the
only documentation source going forward.

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

Mintlify deploys changes from the connected branch. No documentation build or
generated output is committed to this repository.

## Custom domain setup

The production hostname is `docs.lilfella.app`. It is a subdomain of the same
Cloudflare-managed zone that serves `lilfella.app`, so it does not compete with
the Cloudflare Worker or Pages route for the marketing site.

In the Mintlify dashboard:

1. Open **Settings → Custom domain**.
2. Add `docs.lilfella.app`.
3. Copy the verification records Mintlify shows for this deployment.

In Cloudflare DNS, add the two verification TXT records exactly as Mintlify
shows them:

```text
TXT    _acme-challenge.docs.lilfella.app       <value from Mintlify>
TXT    _cf-custom-hostname.docs.lilfella.app   <value from Mintlify>
```

Then add the CNAME:

```text
CNAME  docs.lilfella.app   cname.mintlify.builders
```

Use **DNS only** (the gray cloud) while Mintlify validates the hostname and
provisions TLS. After the domain is live, follow Mintlify's Cloudflare guidance
if proxying is needed. The `docs` hostname must not already have another A,
AAAA, or CNAME record. If the zone has CAA records, allow
`letsencrypt.org` so Mintlify can issue the certificate.

Mintlify manages the certificate after DNS verification. The TXT values are
deployment-specific, so they must come from the Mintlify dashboard rather than
being committed to this repository. See the [Mintlify custom-domain
guide](https://www.mintlify.com/docs/customize/custom-domain) for the current
dashboard flow and provider-specific behavior.

## Marketing-site migration

The `fella-web` repository owns the marketing origin, not the documentation
content. Its migration has four parts:

1. Delete the old `marketing/docs.html` page.
2. Change every marketing navigation link from `/docs` to
   `https://docs.lilfella.app`.
3. Add permanent redirects for `/docs`, `/docs.html`, and `/docs/*` to the
   Mintlify hostname so old bookmarks continue to work.
4. Remove `/docs` from the `lilfella.app` sitemap. Mintlify publishes the
   documentation sitemap on the documentation hostname.

The root marketing site continues to own `/`, `/why`, `/initiatives`, install
redirects, and the paused Packs route. The docs subdomain owns all product and
contributor pages.

## Verification checklist

After the DNS and Mintlify settings are saved:

```sh
dig +short docs.lilfella.app CNAME
curl -I https://docs.lilfella.app
curl -fsSL https://docs.lilfella.app/llms.txt >/dev/null
curl -I https://lilfella.app/docs
```

Expected behavior:

- The CNAME resolves to `cname.mintlify.builders`.
- `https://docs.lilfella.app` loads the Mintlify home page over HTTPS.
- `llms.txt` is served by Mintlify and lists the published pages.
- `https://lilfella.app/docs` returns a permanent redirect to the docs domain.
- No page links back to the retired static guide.

If validation fails, check the exact TXT names and values, remove conflicting
DNS records, confirm the Cloudflare proxy is disabled during validation, and
retry the domain in Mintlify only after DNS is correct.
