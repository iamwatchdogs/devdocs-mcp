# Phase 1 demo — the CLI alone

Date: 2026-11-13. Repo state: `5291fdc`. The task below is answered with
the `devdocs` CLI alone, offline, against the fixture-rebuilt corpus
database. Every command carries the same two flags, so the run reproduces
from a fresh `just eval-corpus`:

```
--db eval/runs/corpus.db --config eval/corpus.toml
```

## Task

How do I rewrite an incoming request path to another destination in
Next.js 16.2.9 (the middleware-style path rewrite)?

## Transcript

### 1. Search

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml --json search "proxy rewrite" --library next.js --version 16.2.9
```

Decisive output lines (top refs, JSON, one line per ref):

```text
docs://next.js/v16.2.9/v/rewrite                  rewrite()
docs://next.js/v16.2.9/r/rewrite                  Rewrite
docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy   Routing requests using proxy
```

Exit 0. Every result carries `served_version: "16.2.9"` and
`version_status: "verified"`.

### 2. Open the best ref

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml open docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy
```

Output (exit 0):

```text
docs (c) Vercel
```

No verbatim page. The corpus boots at `approved_index` (docs/implementation/07
§License-gate state machine): the metadata serves, the verbatim text does
not. The render gate is a design property, not a fault.

### 3. Lift the render gate (operator step)

`approved_snippet` grants verbatim spans. The state machine requires the
adjacent hops, so the demo walks the ladder (docs/implementation/07
§License-gate state machine):

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml --json source approve next.js approved_embed   --operator eval-demo --note "demo: MIT docs, snippet render for the Phase 1 demo"
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml --json source approve next.js approved_cache   --operator eval-demo --note "demo: MIT docs, snippet render for the Phase 1 demo"
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml --json source approve next.js approved_snippet --operator eval-demo --note "demo: MIT docs, snippet render for the Phase 1 demo"
```

Output (exit 0, three lines):

```text
{"expires":null,"from":"approved_index","library":"next.js","note":"demo: MIT docs, snippet render for the Phase 1 demo","operator":"eval-demo","rows":1,"to":"approved_embed"}
{"expires":null,"from":"approved_embed","library":"next.js","note":"demo: MIT docs, snippet render for the Phase 1 demo","operator":"eval-demo","rows":1,"to":"approved_cache"}
{"expires":null,"from":"approved_cache","library":"next.js","note":"demo: MIT docs, snippet render for the Phase 1 demo","operator":"eval-demo","rows":1,"to":"approved_snippet"}
```

### 4. Open again — the page serves

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml --json open docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy --max-tokens 1500
```

Decisive output (exit 0, `token_count: 180`, `license_status:
"approved_snippet"`, `served_version: "16.2.9"`, `version_status:
"verified"`):

````text
[next.js 16.2.9 › V › How to route requests to the right zone › Routing requests using proxy]

Routing requests through [`rewrites`](/docs/app/api-reference/config/next-config-js/rewrites) is recommended to minimize latency overhead for the requests, but proxy can also be used when there is a need for a dynamic decision when routing. For example, if you are using a feature flag to decide where a path should be routed such as during a migration, you can use proxy.

```js title="proxy.js"
export async function proxy(request) {
  const { pathname, search } = request.nextUrl
  if (pathname === '/your-path' && myFeatureFlag.isEnabled()) {
    return NextResponse.rewrite(`${rewriteDomain}${pathname}${search}`)
  }
}
```
````

### 5. Verify the citation before relying on it

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml verify docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy
```

Output (exit 0):

```text
docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy verified (8354b87ef29a430335955df64f055f3765a3b56f7defbfb5745e2a12a089f55b)
```

## Answer

In Next.js 16.2.9 the middleware file convention carries the name
`proxy`. For a dynamic rewrite, export `proxy(request)` from `proxy.js`
at the project root, inspect `request.nextUrl`, and return
`NextResponse.rewrite(<destination>)` when the incoming path matches
(`proxy.js`: `export async function proxy(request)`). For a static
path-to-destination mapping, prefer `rewrites()` in `next.config.js` —
the page states that rewrites minimize latency overhead, and that the
proxy fits a dynamic routing decision such as a feature-flagged
migration. Attribution: `docs (c) Vercel`, content hash
`8354b87ef29a430335955df64f055f3765a3b56f7defbfb5745e2a12a089f55b`,
verified before this answer relies on it.
