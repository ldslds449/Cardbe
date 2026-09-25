<script lang="ts">
  import { Input } from "$lib/components/ui/input/index.js";
  import { cn } from "$lib/utils.js";

  let {
    value = $bindable(""),
    onupdate = null,
    placeholder = "",
    class: className = "",
    disabled = false,
  } = $props();

  let editable = $state(false);
  let old_value: string = value;
  let input_el: HTMLElement | null = $state(null);

  $effect(() => {
    if (editable && input_el) {
      input_el.focus();
    }
  });

  function update_value() {
    if (old_value !== value) {
      if (onupdate !== null) {
        onupdate(value);
      }
    }
    old_value = value;
    editable = false;
  }

  function handle_keydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      update_value();
    } else if (event.key === "Escape") {
      event.preventDefault();
      value = old_value;
      editable = false;
    }
  }

  function handle_focusout(event: FocusEvent) {
    event.preventDefault();
    update_value();
  }
</script>

{#if editable}
  <Input
    type="text"
    {placeholder}
    class="max-w-full"
    bind:value
    bind:ref={input_el}
    onkeydown={handle_keydown}
    onfocusout={handle_focusout}
  />
{:else}
  <div
    role="form"
    class={cn("flex items-center h-full w-full select-none", className)}
    ondblclick={() => {
      if (disabled) return;
      old_value = value;
      editable = true;
    }}
  >
    <p class="text-ellipsis overflow-hidden py-1 max-w-full">
      {value}
    </p>
  </div>
{/if}
