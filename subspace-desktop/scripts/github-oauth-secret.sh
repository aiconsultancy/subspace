# shellcheck shell=bash
# The client secret of Subspace's GitHub OAuth App, which a release build
# compiles in so Sign in with GitHub can use the browser. It lives in the
# login keychain as service subspace-github-oauth, account client-secret,
# never in the repository (see docs/installation.md). Never print it.

# Prints the secret without a newline: SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET
# when set, else the login keychain's item, else nothing.
github_oauth_secret() {
	if [[ -n "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" ]]; then
		printf '%s' "$SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET"
		return 0
	fi
	local secret
	secret="$(security find-generic-password -s subspace-github-oauth -a client-secret -w 2>/dev/null)" || secret=''
	printf '%s' "${secret%$'\n'}"
}

# What to say when a stable release has no secret.
github_oauth_secret_missing() {
	echo 'the GitHub OAuth client secret is missing. Add it to the login keychain with' >&2
	echo '  security add-generic-password -s subspace-github-oauth -a client-secret -w' >&2
	echo '(it prompts for the secret), or set SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET' >&2
}
