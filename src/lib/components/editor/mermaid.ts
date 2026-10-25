import { renderAsync } from "pele/lazy";

export function renderDiagram(source: string, idPrefix: string) {
  return renderAsync(source, { idPrefix, links: false, images: false });
}
