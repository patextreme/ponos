# pi-acp (ACP adapter for the pi coding agent), patched in-repo.
#
# Upstream pi-acp accepts ACP `session/new { mcpServers }` but never wires
# them through to pi, which silently degrades ptah's typed-results contract
# (`result = nil` for every resultSchema script). The carried patch
# (patches/pi-acp-mcp-config.patch) materializes stdio MCP servers into a
# per-session `--mcp-config` file for pi. Upstreaming is out of scope by
# decision, so the source is pinned to one exact rev: bumping the rev
# requires rebasing the patch (see openspec/changes/pi-acp-mcp-wiring).
{
  perSystem = {pkgs, ...}: {
    packages.pi-acp = pkgs.buildNpmPackage {
      pname = "pi-acp";
      version = "0.0.33";

      src = pkgs.fetchFromGitHub {
        owner = "svkozak";
        repo = "pi-acp";
        rev = "d1cffc047ab37a096ee70ca39cfc1de463db8d12";
        hash = "sha256-y8QE91ZbRxzoaV8ITw95OqUEpsxkTI9eicygEF1GUFc=";
      };

      patches = [../patches/pi-acp-mcp-config.patch];

      nodejs = pkgs.nodejs_22;
      npmDepsHash = "sha256-/fX79XucKojL/6gZbK5eizEfrXso8rlTgiHfJffmDuY=";
      npmBuild = "npm run build";

      meta = {
        description = "ACP adapter for the pi coding agent (patched: ACP mcpServers wired through to pi)";
        homepage = "https://github.com/svkozak/pi-acp";
        license = pkgs.lib.licenses.mit;
        mainProgram = "pi-acp";
        platforms = pkgs.lib.platforms.unix;
      };
    };
  };
}
