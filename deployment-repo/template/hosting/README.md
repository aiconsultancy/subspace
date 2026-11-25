# Hosting and deployment

The resources this deployment runs on and the code that creates them: the
fleet VM and its private network (Tailscale or Cloudflare Zero Trust), a
shared IRC server, a hosted Subspace API and its database, DNS, and the CI
that deploys them. Keep infrastructure code (Terraform, Bicep, cloud CLI
scripts) here, with state and variable files that hold secrets out of Git.

Build images and deployments from the commits in `../pins.json`, never from a
moving branch.
