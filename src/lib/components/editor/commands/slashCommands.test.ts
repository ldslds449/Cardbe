import { describe, expect, it } from "vite-plus/test";
import { filterSlashCommands, slashCommands } from "./slashCommands";

describe("slash commands", () => {
  it("registers the initial block commands", () => {
    expect(slashCommands.map((command) => command.id)).toEqual([
      "paragraph",
      "heading-1",
      "heading-2",
      "heading-3",
      "bullet-list",
      "ordered-list",
      "task-list",
      "blockquote",
      "code-block",
      "horizontal-rule",
    ]);
  });

  it("filters commands by labels and keywords", () => {
    expect(filterSlashCommands("h2").map((command) => command.id)).toEqual([
      "heading-2",
    ]);
    expect(filterSlashCommands("todo").map((command) => command.id)).toEqual([
      "task-list",
    ]);
  });
});
