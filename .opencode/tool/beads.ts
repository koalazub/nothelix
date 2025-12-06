import { tool } from "@opencode-ai/plugin";

export default tool({
  description: `Run Beads (bd) commands for issue planning and memory.
  
Common subcommands:
- ready --json       List tasks ready to work on
- create "Title"     Create a new issue
- show <id>          Show issue details
- close <id>         Close an issue
- list               List all issues
- dep add <id> <id>  Add dependency between issues

Always prefer --json flag for structured output.`,
  args: {
    subcommand: tool.schema
      .string()
      .describe(
        "bd subcommand and args, e.g. 'ready --json' or 'create \"Title\" -t task -p 1 --json'"
      ),
  },
  async execute(args) {
    // Use .raw to pass the subcommand through the shell for proper parsing
    const result = await Bun.$`bd ${{ raw: args.subcommand }}`.text();
    return result.trim();
  },
});
