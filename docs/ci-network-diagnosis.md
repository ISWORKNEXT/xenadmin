# Runner network investigation

The first premise was that one Azure mirror or one temporary runner failed. Two fresh CPU-check attempts failed resolving azure.archive.ubuntu.com. Removing that entry did not resolve the gate: the next job failed resolving archive.ubuntu.com before tests. The fail-on-update-error change stopped stale-index package retries, but mirror selection is not a verified DNS fix.

The next observation must distinguish a runner-specific resolver problem, Ubuntu-domain lookup failure, general DNS failure and HTTPS connectivity. The local machine resolves archive/security Ubuntu names; that alone does not prove the hosted runner can.

The independent read-only network workflow compares Ubuntu24.04 and22.04 runners without apt installation. A bounded script records system IPv4 lookup results for several independent domains, direct UDP DNS response codes/counts from two public resolvers, and HTTPS status/error results. It does not alter resolver settings, package sources or host configuration and prints only proxy-variable presence, not values. Workflow success means observation collection completed, not that DNS or HTTPS passed. The per-case JSON determines that result.

The actual diagnostic run43039227934 collected both runners. Each resolves GitHub, Microsoft and PyPI through the default127.0.0.53 stub, but all three Ubuntu names time out. Direct1.1.1.1 queries return successful answers for GitHub and archive Ubuntu on both runners.8.8.8.8 was not consistently usable for archive Ubuntu and is not selected as the fallback. This rules out a single Ubuntu mirror and a general loss of internet access; the underlying reason for the default resolver's selective failure remains unknown.

The next mitigation applies only to ephemeral GitHub-hosted Linux runners and only after the default archive lookup fails. It selects the already-observed1.1.1.1 upstream on the runner's route interface, flushes resolver caches, then requires successful archive/security/GitHub/Microsoft lookups and an HTTPS request before apt installation. Healthy resolvers remain unchanged. Product clients and .170 networking are untouched. Actual workflow verification is pending; the earlier mirror-only mitigation did not pass.

Existing local tests and real .170 qualification remain separate from hosted CI status.
