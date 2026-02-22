#!/bin/bash
# Automatic test coverage analyzer for the C compiler.
# Uses cargo-tarpaulin for real line coverage — fully dynamic, nothing hardcoded.
#
# Usage:
#   ./coverage.sh           Run tests + tarpaulin, show full report
#   ./coverage.sh --quick   Skip tests, analyze from last tarpaulin report
#   ./coverage.sh --help    Show this help

set -euo pipefail
cd "$(dirname "$0")"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

REPORT="target/tmp/tarpaulin-report.json"
THRESHOLD=50  # files below this % get flagged

if [[ "${1:-}" == "--help" ]]; then
    head -8 "$0" | tail -6
    exit 0
fi

QUICK=0
[[ "${1:-}" == "--quick" ]] && QUICK=1

# ─── 1. Run tarpaulin (unless --quick) ───────────────────────────

if [[ $QUICK -eq 0 ]]; then
    if ! command -v cargo-tarpaulin &>/dev/null; then
        echo -e "${RED}cargo-tarpaulin not found.${NC} Install with: cargo install cargo-tarpaulin"
        exit 1
    fi

    echo -e "${BOLD}Running cargo tarpaulin (this takes a minute)...${NC}"
    cargo tarpaulin --out json --output-dir target/tmp --skip-clean 2>&1 | \
        grep -E "^[0-9]+\.[0-9]+% coverage|^running|test result|^\|\|" | tail -5
    echo ""
fi

if [[ ! -f "$REPORT" ]]; then
    echo -e "${RED}No tarpaulin report found at ${REPORT}${NC}"
    echo "Run without --quick first, or run: cargo tarpaulin --out json --output-dir target/tmp"
    exit 1
fi

# ─── 2. Parse tarpaulin JSON into per-file coverage ──────────────

# Use python3 to parse JSON and produce section-delimited output for bash
PARSED=$(python3 - "$REPORT" "$THRESHOLD" <<'PYEOF'
import json, sys, os
from collections import defaultdict

report_path = sys.argv[1]
threshold = int(sys.argv[2])

with open(report_path) as f:
    data = json.load(f)

files = data.get("files", [])
crate_stats = defaultdict(lambda: {"covered": 0, "coverable": 0, "files": 0})
low_files = []
all_files = []

for fobj in files:
    parts = fobj["path"]
    path = os.path.join(*parts) if isinstance(parts, list) else parts

    crate = "unknown"
    for seg in ["codegen", "driver", "ir", "lexer", "model", "optimizer", "parser", "semantic"]:
        if seg in (parts if isinstance(parts, list) else path.split(os.sep)):
            crate = seg
            break

    fname = parts[-1] if isinstance(parts, list) else os.path.basename(path)
    covered = fobj.get("covered", 0)
    coverable = fobj.get("coverable", 0)
    pct = (covered / coverable * 100) if coverable > 0 else 100.0

    crate_stats[crate]["covered"] += covered
    crate_stats[crate]["coverable"] += coverable
    crate_stats[crate]["files"] += 1

    # Find uncovered line ranges
    uncovered = sorted(t["line"] for t in fobj.get("traces", []) if t["stats"].get("Line", 0) == 0)

    # Compress into ranges
    ranges = []
    if uncovered:
        start = end = uncovered[0]
        for ln in uncovered[1:]:
            if ln == end + 1:
                end = ln
            else:
                ranges.append(f"{start}" if start == end else f"{start}-{end}")
                start = end = ln
        ranges.append(f"{start}" if start == end else f"{start}-{end}")

    all_files.append((crate, fname, covered, coverable, pct, ranges))
    if pct < threshold:
        low_files.append((crate, fname, covered, coverable, pct, ranges))

# ── Crate summary ──
print("@@CRATE_SUMMARY@@")
for crate in sorted(crate_stats.keys()):
    s = crate_stats[crate]
    pct = (s["covered"] / s["coverable"] * 100) if s["coverable"] > 0 else 100.0
    print(f"{crate}\t{s['covered']}\t{s['coverable']}\t{s['files']}\t{pct:.1f}")

total_covered = sum(s["covered"] for s in crate_stats.values())
total_coverable = sum(s["coverable"] for s in crate_stats.values())
total_pct = (total_covered / total_coverable * 100) if total_coverable > 0 else 0
print(f"TOTAL\t{total_covered}\t{total_coverable}\t{sum(s['files'] for s in crate_stats.values())}\t{total_pct:.1f}")

# ── Files below threshold ──
print("@@LOW_COVERAGE@@")
for crate, fname, cov, able, pct, ranges in sorted(low_files, key=lambda x: x[4]):
    range_str = ", ".join(ranges[:10])
    if len(ranges) > 10:
        range_str += f" (+{len(ranges)-10} more)"
    print(f"{crate}\t{fname}\t{cov}\t{able}\t{pct:.1f}\t{range_str}")

# ── Well-covered files ──
print("@@HIGH_COVERAGE@@")
high = [(c, f, cov, able, p) for c, f, cov, able, p, _ in all_files if p >= 80 and able > 10]
for crate, fname, cov, able, pct in sorted(high, key=lambda x: -x[4]):
    print(f"{crate}\t{fname}\t{cov}\t{able}\t{pct:.1f}")

# ── Zero-coverage files ──
print("@@ZERO_COVERAGE@@")
for crate, fname, cov, able, pct, ranges in all_files:
    if cov == 0 and able > 0:
        print(f"{crate}\t{fname}\t{able}")

print("@@END@@")
PYEOF
)

# ─── 3. Display line coverage results ────────────────────────────

echo -e "${BOLD}═══ LINE COVERAGE BY CRATE ═══${NC}"
echo ""

crate_section=$(echo "$PARSED" | sed -n '/^@@CRATE_SUMMARY@@$/,/^@@/{/^@@/d;p;}')
while IFS=$'\t' read -r crate covered coverable files pct; do
    [[ -z "$crate" ]] && continue
    if [[ "$crate" == "TOTAL" ]]; then
        echo ""
        echo -e "  ${BOLD}TOTAL: ${pct}% (${covered}/${coverable} lines across ${files} files)${NC}"
    else
        color="$RED"
        (( $(echo "$pct > 30" | bc -l 2>/dev/null || echo 0) )) && color="$YELLOW"
        (( $(echo "$pct > 60" | bc -l 2>/dev/null || echo 0) )) && color="$GREEN"
        printf "  ${color}%9s${NC}: %5.1f%%  (%s/%s lines, %s files)\n" "$crate" "$pct" "$covered" "$coverable" "$files"
    fi
done <<< "$crate_section"

echo ""

# Low coverage files
low_section=$(echo "$PARSED" | sed -n '/^@@LOW_COVERAGE@@$/,/^@@/{/^@@/d;p;}')
if [[ -n "$(echo "$low_section" | tr -d '[:space:]')" ]]; then
    echo -e "${BOLD}═══ FILES BELOW ${THRESHOLD}% COVERAGE ═══${NC}"
    echo ""
    prev_crate=""
    while IFS=$'\t' read -r crate fname covered coverable pct ranges; do
        [[ -z "$crate" ]] && continue
        if [[ "$crate" != "$prev_crate" ]]; then
            [[ -n "$prev_crate" ]] && echo ""
            echo -e "  ${CYAN}$crate/${NC}"
            prev_crate="$crate"
        fi
        printf "    ${RED}%5.1f%%${NC}  %-30s  %s/%s lines\n" "$pct" "$fname" "$covered" "$coverable"
        if [[ -n "$ranges" ]]; then
            echo -e "           ${DIM}uncovered: ${ranges}${NC}"
        fi
    done <<< "$low_section"
    echo ""
fi

# Zero coverage files
zero_section=$(echo "$PARSED" | sed -n '/^@@ZERO_COVERAGE@@$/,/^@@/{/^@@/d;p;}')
if [[ -n "$(echo "$zero_section" | tr -d '[:space:]')" ]]; then
    echo -e "${BOLD}═══ ZERO COVERAGE (never executed) ═══${NC}"
    echo ""
    while IFS=$'\t' read -r crate fname lines; do
        [[ -z "$crate" ]] && continue
        echo -e "  ${RED}$crate/$fname${NC} ($lines coverable lines)"
    done <<< "$zero_section"
    echo ""
fi

# Well-covered files
high_section=$(echo "$PARSED" | sed -n '/^@@HIGH_COVERAGE@@$/,/^@@/{/^@@/d;p;}')
if [[ -n "$(echo "$high_section" | tr -d '[:space:]')" ]]; then
    echo -e "${BOLD}═══ WELL COVERED (≥80%) ═══${NC}"
    echo ""
    while IFS=$'\t' read -r crate fname covered coverable pct; do
        [[ -z "$crate" ]] && continue
        printf "  ${GREEN}%5.1f%%${NC}  %s/%s\n" "$pct" "$crate" "$fname"
    done <<< "$high_section"
    echo ""
fi

# ─── 4. Integration test inventory (dynamic) ─────────────────────

echo -e "${BOLD}═══ INTEGRATION TESTS ═══${NC}"
echo ""

total_integ=$(find testing -maxdepth 1 -name '*.c' 2>/dev/null | wc -l)
total_expect=$(grep -rl '// EXPECT' testing/*.c 2>/dev/null | wc -l)
no_expect=$((total_integ - total_expect))

echo "  $total_integ test programs ($total_expect with EXPECT annotation)"
echo ""

if [[ $no_expect -gt 0 ]]; then
    echo -e "  ${YELLOW}Missing EXPECT annotation:${NC}"
    for f in testing/*.c; do
        if ! grep -q '// EXPECT' "$f" 2>/dev/null; then
            echo "    - $(basename "$f")"
        fi
    done
    echo ""
fi

# Auto-group integration tests by shared filename prefix
declare -A GROUP_COUNT
for f in testing/*.c; do
    base=$(basename "$f" .c)
    tag="${base#test_}"
    IFS='_' read -ra parts <<< "$tag"
    best_key="$tag"
    for (( plen=${#parts[@]}-1; plen>=1; plen-- )); do
        prefix="${parts[*]:0:$plen}"
        prefix="${prefix// /_}"
        # Count files matching this prefix (with _ suffix or exact match)
        match_count=$(find testing -maxdepth 1 -name "test_${prefix}_*.c" -o -name "test_${prefix}.c" 2>/dev/null | wc -l)
        if [[ "$match_count" -ge 2 ]]; then
            best_key="$prefix"
            break
        fi
    done
    GROUP_COUNT["$best_key"]=$(( ${GROUP_COUNT["$best_key"]:-0} + 1 ))
done

echo -e "  ${GREEN}Feature groups:${NC}"
for key in $(for k in "${!GROUP_COUNT[@]}"; do echo "${GROUP_COUNT[$k]} $k"; done | sort -rn | awk '{print $2}'); do
    count=${GROUP_COUNT[$key]}
    [[ $count -lt 2 ]] && continue
    printf "    %-20s %d tests\n" "$key" "$count"
done

singleton=0
for k in "${!GROUP_COUNT[@]}"; do
    [[ ${GROUP_COUNT[$k]} -eq 1 ]] && singleton=$((singleton + 1))
done
echo -e "  ${DIM}+ $singleton features with 1 test each${NC}"

# ─── 5. Summary ──────────────────────────────────────────────────

echo ""
echo -e "${BOLD}═══ SUMMARY ═══${NC}"
echo ""
total_pct=$(echo "$crate_section" | grep '^TOTAL' | cut -f5)
echo "  Line coverage:      ${total_pct}%"
echo "  Integration tests:  ${total_integ} programs (${total_expect} with EXPECT)"
echo ""
