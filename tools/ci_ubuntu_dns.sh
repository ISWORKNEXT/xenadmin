#!/usr/bin/env bash
# Only ephemeral GitHub-hosted Ubuntu runners may change their resolver here.
set -euo pipefail
if [[ "${GITHUB_ACTIONS:-}" != "true" || "${RUNNER_ENVIRONMENT:-}" != "github-hosted" || "${RUNNER_OS:-}" != "Linux" ]]; then
  echo 'This helper only applies to GitHub-hosted Linux runners.' >&2
  exit 1
fi
if timeout 8 getent ahostsv4 archive.ubuntu.com >/dev/null; then
  echo 'Ubuntu package DNS is available; resolver unchanged.'
  exit 0
fi
echo 'Ubuntu lookup failed on the default resolver; trying the independently observed public resolver.'
ci_dns_link=$(ip -o route get 1.1.1.1 | awk '{for(i=1;i<=NF;i++) if($i=="dev"){print $(i+1); exit}}')
test -n "$ci_dns_link"
sudo resolvectl dns "$ci_dns_link" 1.1.1.1
sudo resolvectl domain "$ci_dns_link" '~.'
sudo resolvectl flush-caches
for ci_dns_host in archive.ubuntu.com security.ubuntu.com github.com packages.microsoft.com; do
  timeout 8 getent ahostsv4 "$ci_dns_host" >/dev/null
  echo "Resolved $ci_dns_host after runner fallback."
done
curl --fail --silent --show-error --head --location --connect-timeout 5 --max-time 15 \
  --output /dev/null https://archive.ubuntu.com/ubuntu/
echo 'Runner Ubuntu DNS and HTTPS checks passed.'
