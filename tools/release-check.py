#!/usr/bin/env python3
"""The release rules in .github/workflows/build.yml, checked on every push (the release itself runs rarely).

A release ships exactly what a green run of its commit built and checked: the Flatpak and the Decky plugin from the
reused run, each with its checksum, after the green full builds a release needs (REQUIRED_GREEN_BUILDS); the plugin's
packages are pinned by its lockfile. Also here, because they guard the same promise: one-dot versions, every job on
the runner the repo variable picks, actions pinned by commit, and an engine and helper with no outside crates."""
import glob
import os
import re
import sys

wf = open(".github/workflows/build.yml").read()
problems = []


def need(text, why):
    if text not in wf:
        problems.append(why)


# ---- the version ----
version = open("VERSION").read().splitlines()[0].strip() if os.path.exists("VERSION") else ""
if not re.fullmatch(r"[0-9]+\.[0-9]+", version):
    problems.append(f'VERSION must be a one-dot version like 0.3 or 1.0, not "{version}"')
need("grep -Eq '^[0-9]+\\.[0-9]+$'", "the workflow must refuse a VERSION that isn't a one-dot version")
need("tag_name: v${{ needs.rules.outputs.version }}", "a release is tagged v<VERSION>")

# ---- the plugin's packages ----
need("pnpm install --frozen-lockfile", "the Decky job must install from the committed lockfile")
if "--no-frozen-lockfile" in wf:
    problems.append("the Decky job may not resolve new package versions")

# ---- green builds ----
# One green full build of the commit, a single named number at the top of the workflow.
REQUIRED_GREEN_BUILDS = 1
need(f"\nenv:\n  REQUIRED_GREEN_BUILDS: {REQUIRED_GREEN_BUILDS}\n", f"the workflow must set REQUIRED_GREEN_BUILDS to {REQUIRED_GREEN_BUILDS} near its top")
if wf.count("REQUIRED_GREEN_BUILDS:") != 1:
    problems.append("REQUIRED_GREEN_BUILDS must be set in exactly one place")
need('if [ "$greens" -lt "$REQUIRED_GREEN_BUILDS" ]', "a release needs REQUIRED_GREEN_BUILDS green full builds of its commit")
# An attempt made by "Re-run failed jobs" carries the passed jobs over; it is not a green run of everything.
need("select(.run_attempt != $a)", "only full attempts may count as green builds (not re-runs of the failed jobs alone)")
if "carried" in wf and wf.index('[ "$carried" = 0 ] || continue') > wf.index("greens=$((greens + 1))"):
    problems.append("the full-attempt check must come before a green run is counted")

# ---- the release takes its files from the reused run ----
rel = wf[wf.index("  release:"):] if "  release:" in wf else ""
if not rel:
    problems.append("the workflow has no release job")
FILES = ("TurbineHub.flatpak", "TurbineHub-Decky.zip")
if "run-id: ${{ needs.reuse.outputs.run }}" not in rel or "artifact-ids: ${{ needs.reuse.outputs.ids }}" not in rel:
    problems.append("the release must take both the Flatpak and the Decky zip from the reused green run")
need(f"for name in {' '.join(FILES)}; do", "the reuse step must find both the Flatpak and the Decky zip")
if "needs.reuse.outputs.run != ''" not in rel:
    problems.append("the release must not run without a reused green run")
if "No checksum beside" not in rel or "exit 1" not in rel.split("No checksum beside")[-1][:80]:
    problems.append("a missing checksum must stop the release")
if "::warning title=No checksum" in rel:
    problems.append("a missing checksum only warns")
if "sha256sum -c" not in rel:
    problems.append("the release must check each file against its checksum")
# Turbine Hub's own update check reads the checksum file beside the bundle (not GitHub's API, whose hourly limit can
# refuse it), so the release must publish both checksum files.
for f in FILES:
    for name in (f, f + ".sha256"):
        if ("\n            " + name + "\n") not in rel:
            problems.append(f"the release must publish {name}")
# A download can be checked from two places: the checksum file beside it and the release notes.
if "sha256sum TurbineHub.flatpak TurbineHub-Decky.zip" not in rel or "body_path: release-notes.md" not in rel or "} > release-notes.md" not in rel:
    problems.append("the release notes must carry the SHA-256 of both files")
# No job but the release may write to the repository, and nothing else makes a release or a tag.
if wf.count("contents: write") != 1 or "contents: write" not in rel:
    problems.append("only the release job may have contents: write")
if wf.count("action-gh-release@") != 1 or "gh release create" in wf or "git tag" in wf or "git push" in wf:
    problems.append("only the release job's one step may make a release or a tag")
for job in ("decky", "flatpak"):
    part = wf[wf.index(f"  {job}:"):] if f"  {job}:" in wf else ""
    part = part[:part.index("\n    steps:")] if "\n    steps:" in part else part
    if "needs.reuse.outputs.run == ''" not in part:
        problems.append(f"the {job} job must not build again for a release that reuses a green run")

# ---- commits that change only ignored paths ----
# Such a commit reuses the green build of the commit before it. The walk back must ignore exactly what the push
# trigger ignores (and nothing else), and must run before the green runs are counted.
top = wf[:wf.index("jobs:")]
ignored = re.findall(r'^\s+- "([^"]+)"', top[top.index("paths-ignore:"):top.index("workflow_dispatch:")], re.M) if "paths-ignore:" in top else []
if ignored != ["**/*.md"]:
    problems.append(f"the push trigger's paths-ignore changed ({ignored}): change the release's walk back over ignored commits (its case line) to match, then this list")
walk = 'case "$f" in *.md) ;; *) only_ignored=0 ;; esac'
if walk not in wf:
    problems.append("the release must walk back over commits that change only ignored paths (*.md) to the build they reuse")
elif wf.index(walk) > wf.index("greens=0"):
    problems.append("the walk back over ignored commits must come before the green builds are counted")
if 'sha="$built"' not in wf:
    problems.append("the release must look for green builds of the commit the walk back found")

# ---- where jobs run, and what they run ----
RUNS_ON = """runs-on: ${{ vars.TURBINE_RUNNER == 'local' && fromJSON('["self-hosted", "turbinehub"]') || 'ubuntu-latest' }}"""
jobs = re.findall(r"^  ([a-z][a-z0-9_-]*):\n", wf[wf.index("jobs:"):], re.M)
runs = re.findall(r"^    runs-on: .*$", wf, re.M)
if len(runs) != len(jobs) or any(r.strip() != RUNS_ON for r in runs):
    problems.append("every job must run where the repo variable TURBINE_RUNNER says: " + RUNS_ON)
for use in re.findall(r"uses: (\S+)", wf):
    if not re.fullmatch(r"[A-Za-z0-9_./-]+@[0-9a-f]{40}", use):
        problems.append(f"{use} must be pinned by its full commit")
if wf.count("image: ghcr.io/flathub-infra/flatpak-github-actions:kde-6.11") != wf.count("image:"):
    problems.append("the Flatpak build and the checks run in ghcr.io/flathub-infra/flatpak-github-actions:kde-6.11")
# The screenshot checks cover Game Mode on a handheld, Game Mode on a TV and a Desktop Mode window.
for size in ("1280x800", "1920x1080", "1600x900"):
    shards = re.findall(r"part: " + size + r", shard: (\d+), shards: (\d+)", wf)
    if not shards or sorted(int(s) for s, _ in shards) != list(range(int(shards[0][1]))):
        problems.append(f"the checks must cover {size} with every one of its shards")
# A live service may warn, never fail.
live = wf[wf.index("  live:"):wf.index("  decky:")] if "  live:" in wf and "  decky:" in wf else ""
if "continue-on-error: true" not in live or "exit 1" in live:
    problems.append("the live check may only warn")
if "exit 1" in open("tools/live-check.sh").read():
    problems.append("tools/live-check.sh may only warn")

# ---- the house rules are the same list everywhere ----
words = set(re.findall(r"printf '%s' '([A-Za-z0-9+/=]{20,})' \| base64 -d", wf)) | set(re.findall(r"printf '%s' '([A-Za-z0-9+/=]{20,})' \| base64 -d", open("tools/house-rules.sh").read()))
if len(words) != 1:
    problems.append("the workflow and tools/house-rules.sh must check the same encoded word list")
need('sh tools/house-rules.sh "$range"', "the workflow must check every pushed commit with tools/house-rules.sh")

# ---- no outside crates in the engine or the helper ----


def dependencies(manifest):
    """The names under [dependencies], [build-dependencies] and [dev-dependencies], with each one's line."""
    out, section = {}, ""
    for line in open(manifest).read().splitlines():
        line = line.split("#")[0].strip()
        if line.startswith("["):
            section = line.strip("[]")
        elif "=" in line and section.split(".")[-1] in ("dependencies", "build-dependencies", "dev-dependencies"):
            name, value = line.split("=", 1)
            out[name.strip()] = value.strip()
    return out


for manifest, allowed in (("helper/Cargo.toml", {}), ("engine/Cargo.toml", {"turbinehub-helper": "../helper"})):
    for name, value in dependencies(manifest).items():
        if name not in allowed or f'path = "{allowed[name]}"' not in value:
            problems.append(f"{manifest} depends on {name}: the engine and the helper use the standard library only")

# ---- the interface's functions exist ----
# The app can't be built everywhere this script runs, so a function declared for the interface with no code behind
# it would only show in the Flatpak build. Every #[qinvokable] declared in a bridge needs a function of the same name
# with a body somewhere in app/src, taking the same number of arguments.


def params(text):
    """The parameter list of `fn name(...)` starting at text[0] == '(' (nesting counted), split at top level."""
    depth, out, cur = 0, [], ""
    for ch in text:
        if ch in "(<[":
            depth += 1
            if depth == 1 and ch == "(":
                continue
        elif ch in ")>]":
            depth -= 1
            if depth == 0 and ch == ")":
                break
        if ch == "," and depth == 1:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


sources = {f: open(f).read() for f in sorted(glob.glob("app/src/**/*.rs", recursive=True))}
declared = []
for text in sources.values():
    declared += re.findall(r"#\[qinvokable\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([a-z0-9_]+)\s*(\([^;{]*?\))\s*(?:->[^;{]*)?;", text)
for name, sig in declared:
    want = params(sig)
    shapes = []
    for f, text in sources.items():
        for m in re.finditer(r"\bfn\s+" + name + r"\s*(\([^;{]*?\))\s*(?:->[^;{]*)?\{", text):
            got = params(m.group(1))
            # Other things in the app may carry the same name: only a function that takes self counts.
            if got and "self" in got[0]:
                shapes.append((len(got), f))
    if not shapes:
        problems.append(f"{name} is declared for the interface but no function of that name exists in app/src")
    elif not any(n == len(want) for n, _ in shapes):
        n, f = shapes[0]
        problems.append(f"{name}: the bridge declares {len(want) - 1} arguments, the function in {f} takes {n - 1}")

for p in problems:
    print(f"::error title=Release rules::{p}")
print(f"Release rules: {len(problems)} problems ({len(jobs)} jobs, {len(declared)} interface functions)")
sys.exit(1 if problems else 0)
