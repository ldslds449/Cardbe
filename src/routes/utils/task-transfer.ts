import {
  create_task,
  create_task_item,
  type Recurrence,
  type Task,
} from "../type/task.svelte";

const BINARY_PREFIX = "cardbe-task:v1:";
const BINARY_VERSION = 1;
const MAX_SHARED_TEXT_LENGTH = 2_000_000;
const MAX_DECOMPRESSED_BYTES = 8_000_000;
const MAX_COLLECTION_ITEMS = 100_000;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

class BinaryWriter {
  private chunks: Uint8Array[] = [];

  byte(value: number) {
    this.chunks.push(Uint8Array.of(value));
  }

  uint32(value: number) {
    const bytes = new Uint8Array(4);
    new DataView(bytes.buffer).setUint32(0, value, true);
    this.chunks.push(bytes);
  }

  float64(value: number) {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setFloat64(0, value, true);
    this.chunks.push(bytes);
  }

  string(value: string) {
    const bytes = encoder.encode(value);
    this.uint32(bytes.length);
    this.chunks.push(bytes);
  }

  finish(): Uint8Array {
    const length = this.chunks.reduce(
      (total, chunk) => total + chunk.length,
      0,
    );
    const result = new Uint8Array(length);
    let offset = 0;
    for (const chunk of this.chunks) {
      result.set(chunk, offset);
      offset += chunk.length;
    }
    return result;
  }
}

class BinaryReader {
  private offset = 0;
  constructor(private bytes: Uint8Array) {}

  private take(length: number): Uint8Array {
    if (length < 0 || this.offset + length > this.bytes.length) {
      throw new Error("The task sharing text is incomplete");
    }
    const result = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return result;
  }

  byte(): number {
    return this.take(1)[0];
  }

  uint32(): number {
    const bytes = this.take(4);
    return new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true);
  }

  float64(): number {
    const bytes = this.take(8);
    return new DataView(bytes.buffer, bytes.byteOffset, 8).getFloat64(0, true);
  }

  string(): string {
    return decoder.decode(this.take(this.uint32()));
  }
  done(): boolean {
    return this.offset === this.bytes.length;
  }
}

function encode_task(task: Task): Uint8Array {
  const writer = new BinaryWriter();
  writer.byte(BINARY_VERSION);
  writer.string(task.title);
  writer.string(task.description);
  writer.string(task.color);
  writer.float64(task.start_time?.getTime() ?? Number.NaN);
  writer.float64(task.due_time?.getTime() ?? Number.NaN);
  writer.uint32(task.labels.length);
  task.labels.forEach((label) => writer.string(label));
  writer.uint32(task.items.length);
  task.items.forEach((item) => {
    writer.string(item.text);
    writer.byte(item.completed ? 1 : 0);
  });
  const recurrence_code =
    task.recurrence?.frequency === "daily"
      ? 1
      : task.recurrence?.frequency === "weekly"
        ? 2
        : task.recurrence?.frequency === "monthly"
          ? 3
          : 0;
  writer.byte(recurrence_code);
  if (task.recurrence) writer.uint32(task.recurrence.interval);
  return writer.finish();
}

function decode_task(bytes: Uint8Array): Task {
  const reader = new BinaryReader(bytes);
  if (reader.byte() !== BINARY_VERSION)
    throw new Error("Unsupported task sharing version");
  const title = reader.string();
  const description = reader.string();
  const color = reader.string();
  const start_timestamp = reader.float64();
  const due_timestamp = reader.float64();
  const label_count = reader.uint32();
  if (label_count > MAX_COLLECTION_ITEMS)
    throw new Error("Invalid task label count");
  const labels = Array.from({ length: label_count }, () => reader.string());
  const item_count = reader.uint32();
  if (item_count > MAX_COLLECTION_ITEMS)
    throw new Error("Invalid task checklist count");
  const items = Array.from({ length: item_count }, () => {
    const item = create_task_item(reader.string());
    item.completed = reader.byte() === 1;
    return item;
  });
  const recurrence_code = reader.byte();
  let recurrence: Recurrence | undefined;
  if (recurrence_code !== 0) {
    const frequency =
      recurrence_code === 1
        ? "daily"
        : recurrence_code === 2
          ? "weekly"
          : recurrence_code === 3
            ? "monthly"
            : undefined;
    if (!frequency) throw new Error("Invalid task recurrence");
    const interval = reader.uint32();
    if (interval < 1) throw new Error("Invalid task recurrence");
    recurrence = { frequency, interval };
  }
  if (!reader.done())
    throw new Error("The task sharing text contains unexpected data");

  return create_task(
    "",
    title,
    description,
    color,
    Number.isNaN(start_timestamp) ? undefined : new Date(start_timestamp),
    Number.isNaN(due_timestamp) ? undefined : new Date(due_timestamp),
    labels,
    items,
    recurrence,
  );
}

async function gzip(data: Uint8Array): Promise<Uint8Array> {
  const stream = new CompressionStream("gzip");
  const output = new Response(stream.readable).arrayBuffer();
  const writer = stream.writable.getWriter();
  await writer.write(data);
  await writer.close();
  return new Uint8Array(await output);
}

async function gunzip(data: Uint8Array): Promise<Uint8Array> {
  const stream = new DecompressionStream("gzip");
  const reader = stream.readable.getReader();
  const output = (async () => {
    const chunks: Uint8Array[] = [];
    let length = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > MAX_DECOMPRESSED_BYTES) {
        await reader.cancel();
        throw new Error("Task sharing text is too large");
      }
      chunks.push(value);
    }
    const result = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      result.set(chunk, offset);
      offset += chunk.length;
    }
    return result;
  })();
  const writer = stream.writable.getWriter();
  await writer.write(data);
  await writer.close();
  return output;
}

function to_base64url(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function from_base64url(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(value))
    throw new Error("Invalid task sharing text");
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64.padEnd(Math.ceil(base64.length / 4) * 4, "="));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

export async function serialize_portable_task(task: Task): Promise<string> {
  return BINARY_PREFIX + to_base64url(await gzip(encode_task(task)));
}

export async function parse_portable_task(shared_text: string): Promise<Task> {
  const input = shared_text.trim();
  if (input.length > MAX_SHARED_TEXT_LENGTH)
    throw new Error("Task sharing text is too large");
  if (!input.startsWith(BINARY_PREFIX)) {
    throw new Error("This is not supported Cardbe task sharing text");
  }
  try {
    return decode_task(
      await gunzip(from_base64url(input.slice(BINARY_PREFIX.length))),
    );
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("Invalid task"))
      throw error;
    throw new Error("Invalid or damaged task sharing text");
  }
}
