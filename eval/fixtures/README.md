# Eval fixture corpus

The recorded fixtures the golden suites grade against
(docs/implementation/14 §Fixture corpus). Everything here is committed, so
`just eval-corpus` rebuilds the corpus database fully offline.

## Layout

- `next.js/16.2.9/manifest.json` — recorded responses for the nextjs.org
  `llms.txt` ingest: the index and the 60 pinned pages (their `.md`
  bodies). `inventory.json` sits beside it.
- `zod/4.6.7/manifest.json` — the zod.dev recording: the index and the
  `llms-full.txt` export, which carries the whole doc as one page.
- `serde/1.0.229/` — the git fixture for the serde repository at commit
  `7fc3b4c`: `commit-sha`, `README.md`, `serde_core/README.md`.
- `trim-keep.txt` — the keep list: one URL per line, the index files plus
  every page a golden question pins.
- `trim.py` — the trim tool. Python 3 standard library only.

A manifest is a JSON array of `{url, status, headers, body}` objects, the
shape `devdocs add --record-to` writes. Replay serves a response by exact
URL and fails closed on a miss, so a manifest must record every page its
rewritten index links.

## Recording origin

The fixtures were recorded from the live llms.txt endpoints on
2026-11-13. The next.js recording captured the docs nextjs.org served
that day: the site served its then-current docs under the 16.2.9 corpus
label, because llms.txt serves latest only and has no versioned
endpoint. Real-network re-recording stays manual and labeled.

## serde git fixture

The git fixture is materialized by hand, not recorded: `--record-to`
records llms.txt HTTP responses only and refuses git sources
(docs/implementation/07 §Git adapter, fixture replay).

To materialize `serde/<version>/` (manual, networked — like every
real-network rebuild):

1. Shallow-clone https://github.com/serde-rs/serde and check out the
   pinned commit: the 40-hex SHA recorded in `serde/<version>/commit-sha`
   (currently `7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8`).
2. Copy the files the docs walk keeps into `serde/<version>/` at their
   tree-relative paths. The walk applies the ingest extension allowlist
   (`md`, `mdx`, `rst`, `adoc`, `txt`, `json`, `yaml`, `yml`; config
   `ingest.git_extensions`) after the tree's own `.gitignore`, with
   hidden entries skipped — the serde recording keeps `README.md` and
   `serde_core/README.md`.
3. Write the `git rev-parse HEAD` output of the checked-out clone into
   `serde/<version>/commit-sha`.

Replay validates the recorded SHA shape (40 hex) and, when the requested
pin is a commit SHA, compares the pin against the recorded SHA and fails
on a mismatch. The `commit-sha` file carries no allowlisted extension, so
the walk never ingests it.

## Trim procedure

The trim keeps only the keep-listed responses and rewrites each kept
`llms.txt` index body so it links only kept pages:

- A link to a kept page stays. A link whose `.md` twin is kept is
  retargeted at the twin, so the replay fetches the same markdown body
  the recording buffered through the HTML-to-twin route. Other links drop.
- A kept page no reachable index lists (its listing index was dropped,
  or sits outside the reachable closure) gets its line merged into the
  deepest reachable index whose directory covers the page, from a
  recorded index body or a synthesized minimal line. A page no
  reachable index covers fails the trim.
- An index that would keep zero link lines points at the kept
  `llms-full.txt` export instead; the export stays a probe-only page, so
  the corpus is the one the recording served.

The tool verifies before it writes: every link line target in a rewritten
index resolves to a recorded response, and every kept page is linked by
some rewritten index the replay can reach. Reachability mirrors the
replay's scope queue: the recording's root index (the manifest's first
response) seeds, a reachable index's link lines and a kept page's
`Link: rel="describedby"` header queue further indexes, and only
reachable indexes lend their links. Trim rejects a keep list that drops
an index a kept page's describedby header names — the header is never
rewritten, so the replay would follow it and miss. Re-trimming a trimmed
manifest with the same keep list is the identity.

To rebuild a fixture after a fresh recording (manual, networked — real
network rebuilds stay manual and labeled):

```sh
python3 eval/fixtures/trim.py <recorded>/manifest.json \
    eval/fixtures/trim-keep.txt \
    eval/fixtures/<library>/<version>/manifest.json \
    eval/fixtures/<library>/<version>/inventory.json
```

The offline rebuild of the corpus database is `just eval-corpus`; it runs
the three adds against the committed fixtures with no network.
