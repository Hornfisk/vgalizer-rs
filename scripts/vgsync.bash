# vgsync — two-way tuning sync across vgalizer boxes.
#
# Source this from your shell rc:
#   [ -f ~/repos/vgalizer/scripts/vgsync.bash ]    && . ~/repos/vgalizer/scripts/vgsync.bash
#   [ -f ~/repos/vgalizer-rs/scripts/vgsync.bash ] && . ~/repos/vgalizer-rs/scripts/vgsync.bash
#
# Usage:
#   vgsync          # push local XDG tuning, rebase, push, apply seed to XDG
#
# It:
#   1. lifts local XDG tuning into the repo seed (tune-sync push)
#   2. commits config.json if anything changed
#   3. pulls remote (rebases over local commit if any)
#   4. pushes if we had local changes
#   5. applies merged seed back to XDG (live-reloads into running vgalizer)
#
# Works in bash and zsh. Auto-detects repo at ~/repos/vgalizer or
# ~/repos/vgalizer-rs (arch vs thinkpad naming).

_vg_repo() {
    [ -d "$HOME/repos/vgalizer" ]    && { echo "$HOME/repos/vgalizer";    return 0; }
    [ -d "$HOME/repos/vgalizer-rs" ] && { echo "$HOME/repos/vgalizer-rs"; return 0; }
    return 1
}

vgsync() {
    local repo
    repo="$(_vg_repo)" || { echo "vgsync: no repo found"; return 1; }
    (
        cd "$repo" || return 1
        ./scripts/tune-sync.sh push >/dev/null 2>&1
        local need_push=0
        if ! git diff --quiet config.json; then
            echo "vgsync: local tuning changes, committing"
            git commit -am "tune: auto-sync from $(hostname -s) $(date +%Y-%m-%d)" >/dev/null
            need_push=1
        fi
        echo "vgsync: pulling remote"
        git pull --rebase origin master || { echo "vgsync: pull failed, resolve and re-run"; return 1; }
        if [ "$need_push" = "1" ]; then
            echo "vgsync: pushing"
            git push origin master || return 1
        fi
        ./scripts/tune-sync.sh pull >/dev/null 2>&1
        echo "vgsync: done at $(git rev-parse --short HEAD)"
    )
}
