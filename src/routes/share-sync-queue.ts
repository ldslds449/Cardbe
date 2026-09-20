/**
 * Serialises publishing for one share link while keeping only its newest
 * requested snapshot.  Unlike a simple "in flight" Set, callers receive a
 * promise for their own request; this is important when a board switch must
 * wait for a queued publish rather than merely the publish already running.
 */
export class LatestShareSyncQueue<T> {
    private running = new Set<string>();
    private latest = new Map<string, { value: T; generation: number }>();
    private generations = new Map<string, number>();
    private waiters = new Map<string, Array<{
        generation: number;
        resolve: () => void;
        reject: (error: unknown) => void;
    }>>();

    constructor(
        private readonly run: (id: string, value: T, is_current: () => boolean) => Promise<void>,
        private readonly on_start: (id: string) => void,
        private readonly on_error: (id: string, error: unknown) => void,
    ) {}

    request(id: string, value: T): Promise<void> {
        const generation = (this.generations.get(id) ?? 0) + 1;
        this.generations.set(id, generation);
        this.latest.set(id, { value, generation });

        const completion = new Promise<void>((resolve, reject) => {
            const waiters = this.waiters.get(id) ?? [];
            waiters.push({ generation, resolve, reject });
            this.waiters.set(id, waiters);
        });

        if (!this.running.has(id)) {
            this.running.add(id);
            void this.drain(id);
        }
        return completion;
    }

    /** Drop queued work when a share is revoked or deleted. */
    retire(id: string) {
        this.latest.delete(id);
        // Keep generations monotonic. A late publish from before a revoke
        // must not save over an explicitly recreated link with the same ID.
        this.generations.set(id, (this.generations.get(id) ?? 0) + 1);
        this.settle_through(id, Number.POSITIVE_INFINITY);
    }

    private settle_through(id: string, generation: number, error?: unknown) {
        const waiters = this.waiters.get(id) ?? [];
        const remaining = waiters.filter((waiter) => {
            if (waiter.generation > generation) return true;
            if (error === undefined) waiter.resolve();
            else waiter.reject(error);
            return false;
        });
        if (remaining.length) this.waiters.set(id, remaining);
        else this.waiters.delete(id);
    }

    private async drain(id: string) {
        try {
            while (true) {
                const next = this.latest.get(id);
                if (!next) return;
                this.latest.delete(id);
                this.on_start(id);
                try {
                    await this.run(id, next.value, () => this.generations.get(id) === next.generation);
                    this.settle_through(id, next.generation);
                } catch (error) {
                    // A revoke can retire this ID and a later explicit action
                    // can recreate it before an older publish rejects. That
                    // rejection belongs to the retired generation, not the
                    // newly-created share.
                    if (this.generations.get(id) === next.generation) {
                        this.on_error(id, error);
                    }
                    this.settle_through(id, next.generation, error);
                    // A newer request may have arrived while the failed one
                    // was running. Let it have its own terminal result.
                    if (!this.latest.has(id)) return;
                }
            }
        } finally {
            this.running.delete(id);
            // There is no await between the final queue check and here, but
            // retain this guard to make future changes race-safe.
            if (this.latest.has(id) && !this.running.has(id)) {
                this.running.add(id);
                void this.drain(id);
            }
        }
    }
}
