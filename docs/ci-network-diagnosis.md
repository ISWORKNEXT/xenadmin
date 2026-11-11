# Runner network investigation

The first premise was that one Azure mirror or one temporary runner failed. Two fresh CPU-check attempts failed resolving azure.archive.ubuntu.com. Removing that entry did not resolve the gate: the next job failed resolving archive.ubuntu.com before tests. The fail-on-update-error change stopped stale-index package retries, but mirror selection is not a verified DNS fix.

The next observation must distinguish a runner-specific resolver problem, Ubuntu-domain lookup failure, general DNS failure and HTTPS connectivity. The local machine resolves archive/security Ubuntu names; that alone does not prove the hosted runner can.

The independent read-only network workflow compares Ubuntu24.04 and22.04 runners without apt installation. A bounded script records system IPv4 lookup results for several independent domains, direct UDP DNS response codes/counts from two public resolvers, and HTTPS status/error results. It does not alter resolver settings, package sources or host configuration and prints only proxy-variable presence, not values. Workflow success means observation collection completed, not that DNS or HTTPS passed. The per-case JSON determines that result.

No subsequent DNS fix is justified until these observations return. Existing local tests and real .170 qualification remain separate from hosted CI status.
