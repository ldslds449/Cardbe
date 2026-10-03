export function archive_activity(
  times: Date[],
  year: number,
  now = new Date(),
) {
  const counts = new Map<string, number>();
  for (const time of times) {
    if (time.getFullYear() === year && time <= now) {
      const key = time.toDateString();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  const start = new Date(year, 0, 1);
  start.setDate(start.getDate() - start.getDay());
  const end = new Date(year, 11, 31);
  end.setDate(end.getDate() + 6 - end.getDay());
  const days = [];
  let total = 0;
  let active_days = 0;
  let streak = 0;
  let longest_streak = 0;
  for (
    const date = new Date(start);
    date <= end;
    date.setDate(date.getDate() + 1)
  ) {
    const in_year = date.getFullYear() === year;
    const available = in_year && date <= now;
    const count = available ? (counts.get(date.toDateString()) ?? 0) : 0;
    days.push({ date: new Date(date), count, in_year, available });
    total += count;
    if (count > 0) {
      active_days++;
      streak++;
      longest_streak = Math.max(longest_streak, streak);
    } else {
      streak = 0;
    }
  }
  return { days, total, active_days, longest_streak };
}
