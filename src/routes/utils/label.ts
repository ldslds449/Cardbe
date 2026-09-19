export type LabelKind =
    | "owner"
    | "category"
    | "priority"
    | "status"
    | "effort"
    | "general";

export interface ParsedLabel {
    kind: LabelKind;
    value: string;
}

/**
 * Recognizes the reserved label namespaces.
 * Unknown or incomplete prefixes remain ordinary labels so user data is never
 * hidden or rewritten unexpectedly.
 */
export function parse_label(label: string): ParsedLabel {
    const separator = label.indexOf(":");
    if (separator < 0) return { kind: "general", value: label };

    const prefix = label.slice(0, separator).trim().toLowerCase();
    const value = label.slice(separator + 1).trim();
    if (!value) return { kind: "general", value: label };

    if (prefix === "owner") return { kind: "owner", value };
    if (prefix === "type") return { kind: "category", value };
    if (prefix === "priority") return { kind: "priority", value };
    if (prefix === "status") return { kind: "status", value };
    if (prefix === "effort") return { kind: "effort", value };
    return { kind: "general", value: label };
}
