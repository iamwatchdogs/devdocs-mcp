#!/usr/bin/env python3
"""Trim a recorded response manifest to a keep-list for offline replay.

Reads a recorded manifest.json (a JSON array of {url, status, headers,
body} objects, the devdocs --record-to shape), keeps only the responses
whose URL is listed in the keep-list file, and rewrites every kept
llms.txt index body so it links only kept pages: the replay then fetches
only listed pages, and every link resolves to a recorded response.

The rewrite, per index body:

- A link line whose target is kept stays as written.
- A link line whose target is a dropped page but whose markdown twin
  (`<target>.md`) is kept is retargeted at the twin; the line keeps its
  title and description, and the replay fetches the markdown body
  directly — the same document the recording buffered through the
  HTML-to-twin route.
- A link line whose target is neither kept nor a kept twin is dropped.
- Kept pages that no kept index links (their listing index was dropped)
  get their line merged in from a recorded index body, retargeted at the
  kept page; a page no recorded index links is synthesized as
  `- [<segment>](<url>)`. Link titles and descriptions never reach the
  corpus, so a merged or synthesized line serves exactly the page it
  names.
- When a rewritten index would hold zero link lines — an index the
  ingest parser refuses — and the keep list holds the scope's
  llms-full.txt export, the rewrite points the index at that export with
  one link line. The export stays a probe-only page (never a page URL),
  so the replay corpus is the one the recording served.

The tool verifies before writing: every link line target in a rewritten
index resolves to a kept response, and every kept page URL is linked by
some rewritten index. Re-trimming a trimmed manifest with the same keep
list is the identity.

Usage:
    python3 trim.py <manifest.json> <keep-list.txt> <out-manifest.json> <out-inventory.json>

The inventory JSON records, per kept URL: the recorded status, the
original body SHA-256, the trimmed SHA-256 when the body changed, and a
trim note. Standard library only; Python 3.9+.
"""

import hashlib
import json
import sys
from urllib.parse import urljoin

USAGE = "usage: python3 trim.py <manifest.json> <keep-list.txt> <out-manifest.json> <out-inventory.json>"


def fail(message):
    print(f"trim: {message}", file=sys.stderr)
    sys.exit(2)


def read_keep_set(path):
    try:
        with open(path, encoding="utf-8") as handle:
            keep = {line.strip() for line in handle if line.strip()}
    except OSError as error:
        fail(f"cannot read {path}: {error}")
    if not keep:
        fail(f"{path}: the keep list is empty")
    return keep


def is_index_url(url):
    """The llms.txt index files (docs/07: the export is not an index)."""
    file = url.split("?", 1)[0].rstrip("/").rsplit("/", 1)[-1]
    return file.lower() == "llms.txt"


def is_page_url(url):
    """Page shapes the ingest can buffer: markdown, HTML, or an
    extensionless directory-style path; never llms.txt / llms-full.txt.
    Mirrors the Rust `is_page_url`."""
    path = url.split("?", 1)[0]
    file = path.rstrip("/").rsplit("/", 1)[-1]
    if path.endswith("/"):
        return True
    stem, dot, ext = file.rpartition(".")
    if not dot:
        return True
    if stem.lower() in ("llms", "llms-full") and ext.lower() == "txt":
        return False
    return ext.lower() in ("md", "mdx", "html", "htm", "xhtml")


def parse_link_line(trimmed):
    """One link line: `- [title](url)` or `* [title](url)`, optional
    `: description`. The destination ends at a depth-zero `)`; balanced
    parentheses are legal URL characters. Mirrors the Rust parser."""
    rest = None
    for bullet in ("- [", "* ["):
        if trimmed.startswith(bullet):
            rest = trimmed[len(bullet):]
            break
    if rest is None:
        return None
    close_bracket = rest.find("](")
    if close_bracket < 0:
        return None
    title = rest[:close_bracket]
    after = rest[close_bracket + 2:]
    depth = 0
    for index, char in enumerate(after):
        if char == "(":
            depth += 1
        elif char == ")":
            if depth == 0:
                if title.strip() and after[:index].strip():
                    return after[:index]
                return None
            depth -= 1
    return None


def fence_marker(trimmed):
    """A code-fence marker: a run of >= 3 backticks or tildes."""
    if not trimmed or trimmed[0] not in "`~":
        return None
    run = len(trimmed) - len(trimmed.lstrip(trimmed[0]))
    return (trimmed[0], run) if run >= 3 else None


def fence_closes(open_fence, trimmed):
    """A fence closes on a run of the same character at least as long,
    holding nothing else (CommonMark, as the Rust parser implements it)."""
    marker = fence_marker(trimmed)
    return (
        marker is not None
        and marker[0] == open_fence[0]
        and marker[1] >= open_fence[1]
        and set(trimmed) == {open_fence[0]}
    )


def link_lines(url, body):
    """Yields (line_index, written_target, resolved_target) for every
    link line outside code fences, mirroring the Rust parser's walk."""
    open_fence = None
    for index, line in enumerate(body.splitlines()):
        trimmed = line.strip()
        if open_fence is None:
            marker = fence_marker(trimmed)
            if marker is not None:
                open_fence = marker
                continue
            written = parse_link_line(trimmed)
            if written is not None:
                yield index, written, urljoin(url, written)
        elif fence_closes(open_fence, trimmed):
            open_fence = None


def body_ends_inside_fence(body):
    """Whether the last fence marker opened an unclosed block."""
    open_fence = None
    for line in body.splitlines():
        trimmed = line.strip()
        if open_fence is None:
            marker = fence_marker(trimmed)
            if marker is not None:
                open_fence = marker
        elif fence_closes(open_fence, trimmed):
            open_fence = None
    return open_fence is not None


def retarget(line, written, url):
    """Rewrites one link line's destination to `url`, keeping the title
    and description."""
    head, sep, tail = line.partition(f"({written})")
    if not sep:
        fail(f"internal: cannot retarget link line: {line!r}")
    return f"{head}({url}){tail}"


def rewrite_index(url, body, keep):
    """Applies the drop/retarget rewrite; returns the new body, the
    dropped count, the retargeted count, and the linked kept targets."""
    drop = set()
    retargets = {}
    written_of = {}
    linked = set()
    for index, written, resolved in link_lines(url, body):
        if resolved in keep:
            linked.add(resolved)
        elif resolved + ".md" in keep:
            retargets[index] = resolved + ".md"
            written_of[index] = written
            linked.add(resolved + ".md")
        else:
            drop.add(index)
    if not drop and not retargets:
        return body, 0, 0, linked
    out_lines = []
    for index, line in enumerate(body.splitlines()):
        if index in drop:
            continue
        out_lines.append(retarget(line, written_of[index], retargets[index]) if index in retargets else line)
    new_body = "\n".join(out_lines)
    if body.endswith("\n") or out_lines:
        new_body += "\n"
    return new_body, len(drop), len(retargets), linked


def find_export(index_url, output_urls):
    """The kept llms-full.txt export in the index's directory, if any."""
    export = index_url.rsplit("/", 1)[0] + "/llms-full.txt"
    return export if export in output_urls else None


def main():
    if len(sys.argv) != 5:
        fail(USAGE)
    manifest_path, keep_path, out_manifest, out_inventory = sys.argv[1:5]
    keep = read_keep_set(keep_path)
    try:
        with open(manifest_path, encoding="utf-8") as handle:
            recorded = json.load(handle)
    except (OSError, ValueError) as error:
        fail(f"cannot read {manifest_path}: {error}")
    if not isinstance(recorded, list) or not recorded:
        fail(f"{manifest_path}: expected a non-empty JSON array of recorded responses")

    seen = set()
    for response in recorded:
        if not isinstance(response, dict) or "url" not in response or "body" not in response:
            fail(f"{manifest_path}: a recorded response lacks url or body")
        if response["url"] in seen:
            fail(f"{manifest_path}: duplicate recorded URL {response['url']}")
        seen.add(response["url"])

    kept = [response for response in recorded if response["url"] in keep]
    missing = sorted(keep - seen)
    if missing:
        print(
            f"trim: note: {len(missing)} keep-list URL(s) are not in this manifest "
            "(a multi-source keep list): " + ", ".join(missing[:3])
            + ("..." if len(missing) > 3 else ""),
            file=sys.stderr,
        )
    if not kept:
        fail(f"{manifest_path}: no recorded URL is in the keep list")

    output_urls = {response["url"] for response in kept}
    # Line sources for pages no kept index links: every recorded index
    # body of this manifest, kept or dropped (docs/07: the subpath
    # listings are indexes too).
    index_bodies = [
        (response["url"], response["body"])
        for response in recorded
        if is_index_url(response["url"])
    ]

    linked = set()
    rewritten_indexes = []  # mutable [response, note, body] records
    for response in kept:
        url = response["url"]
        body = response["body"]
        note = "kept unchanged"
        if is_index_url(url):
            body, dropped, retargeted, targets = rewrite_index(url, body, keep)
            linked |= targets
            parts = []
            if dropped:
                parts.append(f"dropped {dropped} link line(s) to dropped pages")
            if retargeted:
                parts.append(f"retargeted {retargeted} link line(s) at kept .md twins")
            if parts:
                note = "index rewritten: " + "; ".join(parts)
            rewritten_indexes.append([response, note, body])

    # Pages no rewritten index links: merge their line from a recorded
    # index body, or synthesize one (titles never reach the corpus).
    unlinked = sorted(u for u in output_urls if is_page_url(u) and u not in linked)
    if unlinked:
        if not rewritten_indexes:
            fail(f"{manifest_path}: kept pages are unlinked but the manifest records no index to rewrite")
        host = rewritten_indexes[0]
        if body_ends_inside_fence(host[2]):
            fail(f"{host[0]['url']}: the index body ends inside an unclosed code fence; cannot merge link lines")
        merged, synthesized, extra = 0, 0, []
        for page in unlinked:
            line = find_line_for(page, index_bodies)
            if line is None:
                line = f"- [{page.rstrip('/').rsplit('/', 1)[-1]}]({page})"
                synthesized += 1
            else:
                merged += 1
            extra.append(line)
            linked.add(page)
        host[2] = host[2].rstrip("\n") + "\n" + "\n".join(extra) + "\n"
        host[1] = _extend_note(
            host[1],
            f"merged {merged} link line(s) from dropped listings, synthesized {synthesized} "
            f"(kept pages no kept index links)",
        )
        print(f"trim: linked {len(unlinked)} kept page(s) no kept index listed", file=sys.stderr)

    # A rewritten index that would parse empty points at the export.
    for record in rewritten_indexes:
        response, note, body = record
        if not any(True for _ in link_lines(response["url"], body)):
            export = find_export(response["url"], output_urls)
            if export is None:
                fail(
                    f"{response['url']}: the rewrite leaves an empty index and no kept "
                    "llms-full.txt export to point at"
                )
            body = body.rstrip("\n") + f"\n- [Full documentation]({export})\n"
            linked.add(export)
            note = _extend_note(note, "added the kept llms-full.txt export link (a bare index parses empty)")
            record[1], record[2] = note, body

    # Replay consistency: every index link resolves to a kept response,
    # and every kept page is reachable from some rewritten index.
    for response, _, body in rewritten_indexes:
        for _, _, resolved in link_lines(response["url"], body):
            if resolved not in output_urls:
                fail(
                    f"{response['url']}: rewritten index links {resolved}, "
                    "which the trimmed manifest does not record"
                )
    unreachable = sorted(u for u in output_urls if is_page_url(u) and u not in linked)
    if unreachable:
        fail("kept pages no rewritten index links: " + ", ".join(unreachable[:5]))

    inventory = []
    for response in kept:
        body = next((record[2] for record in rewritten_indexes if record[0] is response), response["body"])
        original_sha = hashlib.sha256(response["body"].encode("utf-8")).hexdigest()
        trimmed_sha = None
        if body != response["body"]:
            trimmed_sha = hashlib.sha256(body.encode("utf-8")).hexdigest()
            response["body"] = body
        note = next((record[1] for record in rewritten_indexes if record[0] is response), "kept unchanged")
        inventory.append(
            {
                "url": response["url"],
                "status": response.get("status"),
                "original_sha256": original_sha,
                "trimmed_sha256": trimmed_sha,
                "note": note,
            }
        )

    write_json(out_manifest, kept)
    write_json(
        out_inventory,
        {
            "recorded_manifest": manifest_path,
            "keep_list": keep_path,
            "responses": inventory,
        },
    )
    rewritten = sum(1 for entry in inventory if entry["trimmed_sha256"] is not None)
    print(f"trim: kept {len(kept)}/{len(recorded)} response(s); rewrote {rewritten} body(ies)")


def _extend_note(note, addition):
    return f"{note}; {addition}" if note != "kept unchanged" else addition


def find_line_for(page, index_bodies):
    """The recorded link line naming `page` (directly or through its HTML
    twin), taken from any index body, retargeted at the kept URL."""
    for url, body in index_bodies:
        for _, written, resolved in link_lines(url, body):
            if resolved == page or resolved + ".md" == page:
                for line in body.splitlines():
                    if parse_link_line(line.strip()) == written:
                        return retarget(line, written, page)
    return None


def write_json(path, payload):
    try:
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
            handle.write("\n")
    except OSError as error:
        fail(f"cannot write {path}: {error}")


if __name__ == "__main__":
    main()
