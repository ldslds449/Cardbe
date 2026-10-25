<script lang="ts">
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import TaskActionItems from "../task_action_items.svelte";
  let {
    context = false,
    editable = true,
  }: { context?: boolean; editable?: boolean } = $props();
  const Root = $derived(context ? ContextMenu.Root : DropdownMenu.Root);
  const Content = $derived(
    context ? ContextMenu.Content : DropdownMenu.Content,
  );
  const action = () => {};
</script>

<Root open>
  <Content forceMount portalProps={{ disabled: true }}>
    <TaskActionItems
      {context}
      onView={action}
      onEdit={editable ? action : undefined}
      onArchive={editable ? action : undefined}
      onDelete={editable ? action : undefined}
    />
  </Content>
</Root>
