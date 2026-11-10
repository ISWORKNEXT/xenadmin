# Contributing

Application code lives in `desktop`. Use TypeScript for the interface and Rust for host connections and operations. Do not reintroduce a C# or .NET runtime.

Run the checks listed in README before submitting changes. Verify host behavior against a disposable test environment when changing XenAPI calls. Keep passwords, session references and raw authenticated RPC requests out of files and logs.

Represent incomplete features explicitly. A successful build does not establish compatibility with every XenServer or XCP-ng version.
