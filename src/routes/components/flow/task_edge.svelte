<script lang="ts">
  import { untrack } from "svelte";
  import {
    BaseEdge,
    getBezierPath,
    useInternalNode,
    type EdgeProps,
  } from "@xyflow/svelte";
  import { card_edge_point, type RelationshipEdge } from "./flow";
  let {
    id,
    source,
    target,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    markerStart,
    markerEnd,
    style,
    interactionWidth,
  }: EdgeProps<RelationshipEdge> = $props();
  // An edge's identity includes its source and target task IDs.
  const source_node = useInternalNode(untrack(() => source));
  const target_node = useInternalNode(untrack(() => target));
  const source_point = $derived.by(() => {
    const node = source_node.current;
    return card_edge_point(
      sourceX,
      sourceY,
      sourcePosition,
      node
        ? {
            ...node.internals.positionAbsolute,
            width: node.measured.width ?? node.width ?? 0,
            height: node.measured.height ?? node.height ?? 0,
          }
        : undefined,
    );
  });
  const target_point = $derived.by(() => {
    const node = target_node.current;
    return card_edge_point(
      targetX,
      targetY,
      targetPosition,
      node
        ? {
            ...node.internals.positionAbsolute,
            width: node.measured.width ?? node.width ?? 0,
            height: node.measured.height ?? node.height ?? 0,
          }
        : undefined,
    );
  });
  const [path] = $derived(
    getBezierPath({
      sourceX: source_point.x,
      sourceY: source_point.y,
      targetX: target_point.x,
      targetY: target_point.y,
      sourcePosition,
      targetPosition,
    }),
  );
</script>

<BaseEdge {id} {path} {style} {markerStart} {markerEnd} {interactionWidth} />
