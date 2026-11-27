#!/usr/bin/env bash
# Sourced by terminal-deps.sh and prefetch-zig-deps.sh: how they write into a
# cache that other runs may be writing at the same time. Each writer stages
# under a name holding its own pid, then publishes with a rename that never
# replaces what another writer published first.

# A new staging file (or, with -d, directory) in DIR, owned by this shell.
stage() {
    if [[ "$1" == -d ]]; then
        mktemp -d "$2/.stage-$$-XXXXXX"
    else
        mktemp "$1/.stage-$$-XXXXXX"
    fi
}

# Move the staged SRC to DST unless DST exists; when it does, another writer
# published the same content first, so SRC is discarded and DST kept. Perl
# calls rename(2) for a directory, which fails on a non-empty destination
# instead of moving SRC inside it as mv does, and link(2) for a file, which
# fails on any existing destination, where rename(2) would replace it.
publish() {
    local status=0
    perl -e '
        my ($src, $dst) = @ARGV;
        exit 3 if -e $dst || -l $dst;
        if (-d $src) {
            exit 0 if rename $src, $dst;
            exit 3 if $!{ENOTEMPTY} || $!{EEXIST};
        } else {
            if (link $src, $dst) { unlink $src; exit 0 }
            exit 3 if $!{EEXIST};
        }
        print STDERR "cannot publish $src as $dst: $!\n";
        exit 1;
    ' "$1" "$2" || status=$?
    case "$status" in
    0) ;;
    3) rm -rf "$1" ;;
    *) return 1 ;;
    esac
}

# Remove the staging paths in each DIR whose owner has exited; a live
# writer's are left alone. Best-effort: an orphaned download or unpacking of
# a dead owner may still be writing, and what is left goes on a later run.
remove_dead_stages() {
    local dir path owner
    for dir in "$@"; do
        for path in "$dir"/.stage-*; do
            [[ -e "$path" ]] || continue
            owner="${path##*/.stage-}"
            owner="${owner%%-*}"
            [[ "$owner" =~ ^[0-9]+$ ]] || continue
            kill -0 "$owner" 2>/dev/null || rm -rf "$path" 2>/dev/null || true
        done
    done
}
