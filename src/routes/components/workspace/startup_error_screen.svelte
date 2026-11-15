<script lang="ts">
  import { translateCommandError } from "#lib/command-errors.js";
  import { m } from "#lib/paraglide/messages.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import BugIcon from "@lucide/svelte/icons/bug";

  let { error }: { error: unknown } = $props();
</script>

<main
  class="grid min-h-0 flex-1 place-items-center bg-muted/30 p-6"
  aria-labelledby="startup-error-title"
>
  <ScrollArea
    class="w-full max-w-2xl [&_[data-slot=scroll-area-viewport]]:max-h-[calc(100vh-3rem)]"
  >
    <Card.Root
      class="w-full gap-0 overflow-hidden border-border bg-card p-0 shadow-sm"
    >
      <header class="flex items-start gap-4 border-b px-6 py-6">
        <div
          class="flex size-11 shrink-0 items-center justify-center rounded-xl bg-destructive/10 text-destructive ring-1 ring-inset ring-destructive/20"
          aria-hidden="true"
        >
          <BugIcon class="size-5" />
        </div>
        <div class="min-w-0 space-y-1.5">
          <p
            class="text-xs font-medium uppercase tracking-wider text-muted-foreground"
          >
            {m.ui_local_database()}
          </p>
          <h1
            id="startup-error-title"
            class="text-lg font-semibold tracking-tight"
          >
            {m.ui_couldn_t_open_your_data()}
          </h1>
          <p class="text-sm leading-relaxed text-muted-foreground">
            {m.startup_database_description()}
          </p>
        </div>
      </header>

      <Card.Content class="grid gap-3 py-5">
        <div class="flex items-center gap-2 text-sm font-medium">
          <span class="size-2 rounded-full bg-destructive" aria-hidden="true"
          ></span>
          {m.ui_error_details()}
        </div>
        <ScrollArea
          class="rounded-lg border border-destructive/20 bg-muted/50 [&_[data-slot=scroll-area-viewport]]:max-h-[40vh]"
        >
          <pre
            class="whitespace-pre-wrap break-all px-4 py-3 font-mono text-xs leading-relaxed text-foreground selection:bg-primary/20"
            role="alert">{translateCommandError(error)}</pre>
        </ScrollArea>
        <p class="text-xs text-muted-foreground">
          {m.ui_this_error_is_also_recorded_in_the_cardbe_app_log()}
        </p>
      </Card.Content>
    </Card.Root>
  </ScrollArea>
</main>
