#!/usr/bin/env bash
# Analyze all orphaned branches vs main to determine completeness
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
MAIN="main"

analyze_branch() {
    local branch="$1"
    local ahead=$(git rev-list --count "$branch" ^"$MAIN" 2>/dev/null || echo 0)
    local behind=$(git rev-list --count "$MAIN" ^"$branch" 2>/dev/null || echo 0)
    local ancestor=$(git merge-base "$branch" "$MAIN" 2>/dev/null)
    local hash=$(git rev-parse "$branch" 2>/dev/null || echo "unknown")
    local msg=$(git log --oneline -1 "$branch" 2>/dev/null || echo "unknown")
    local files=$(git diff --stat "$MAIN".."$branch" 2>/dev/null | tail -1 | awk '{print $1}')
    local insertions=$(git diff --stat "$MAIN".."$branch" 2>/dev/null | tail -1 | awk '{print $4}' | tr -d ',' || echo 0)
    local deletions=$(git diff --stat "$MAIN".."$branch" 2>/dev/null | tail -1 | awk '{print $6}' | tr -d ',' || echo 0)

    # Check if already merged into main
    local merged=false
    if git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        merged=true
    fi

    # Check if content exists elsewhere
    local content_unique=true

    echo "---"
    echo "branch: $branch"
    echo "sha: $hash"
    echo "message: ${msg:0:80}"
    echo "merged: $merged"
    echo "ahead: $ahead"
    echo "behind: $behind"
    echo "files_changed: ${files:-0}"
    echo "insertions: ${insertions:-0}"
    echo "deletions: ${deletions:-0}"
    echo "first_file: $(git diff --name-only "$MAIN".."$branch" 2>/dev/null | head -5 | tr '\n' ' ')"
}

echo "# Branch Analysis Report"
echo "# Generated: $(date)"
echo "#"
echo "# Analyzing all branches not in main"

# Analyze audit branches
echo ""
echo "## AUDIT BRANCHES"
echo "These are curriculum content completion branches. They fill in"
echo "remaining template recipes for specific curriculum units."
echo ""

for branch in $(git branch | grep 'audit/' | sed 's/^[ *]*//'); do
    if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        analyze_branch "$branch"
    fi
done

echo ""
echo "## CODEX BRANCHES"
echo "These are codex agent branches - targeted fixes and quality work."
echo ""

for branch in $(git branch | grep 'codex/' | sed 's/^[ *]*//' | grep -v 'ai-review-protocol'); do
    if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        analyze_branch "$branch"
    fi
done

echo ""
echo "## REVIEW BRANCHES"
echo "These are independent review branches for symbolic content."
echo ""

for branch in $(git branch | grep 'review/' | sed 's/^[ *]*//'); do
    if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        analyze_branch "$branch"
    fi
done

echo ""
echo "## FIX BRANCHES"
echo "Targeted fix branches (not codex)."
echo ""

for branch in $(git branch | grep 'fix/' | sed 's/^[ *]*//'); do
    if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        analyze_branch "$branch"
    fi
done

echo ""
echo "## OTHER BRANCHES (external, quality)"
echo ""

for branch in $(git branch | grep -E '(external/|quality/)' | sed 's/^[ *]*//'); do
    if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
        analyze_branch "$branch"
    fi
done

echo ""
echo "# SUMMARY"
echo ""

total_orphaned=0
for prefix in audit codex review fix external quality; do
    count=0
    for branch in $(git branch | grep "^$prefix/" | sed 's/^[ *]*//'); do
        if ! git merge-base --is-ancestor "$branch" "$MAIN" 2>/dev/null; then
            count=$((count + 1))
        fi
    done
    echo "${prefix}: ${count} orphaned branches"
    total_orphaned=$((total_orphaned + count))
done
echo "total: ${total_orphaned} orphaned branches needing evaluation"