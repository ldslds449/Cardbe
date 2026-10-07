import { describe, expect, it } from "vite-plus/test";
import { m } from "$lib/paraglide/messages.js";
import { applyLanguagePreference, getLocale } from "./locale.svelte";
import {
  getLocale as runtimeGetLocale,
  overwriteGetLocale,
} from "$lib/paraglide/runtime.js";
import {
  parse_requested_share_link,
  build_share_snapshot,
} from "../../routes/share";
import { parse_portable_task } from "../../routes/utils/task-transfer";
import { create_card_reference } from "../../routes/utils/card-reference";
import { updateCheckErrorMessage } from "../../routes/utils/update";
import { formatDate, formatDateTime, formatNumber } from "./format";
import en from "../../../messages/en.json";
import zh from "../../../messages/zh-TW.json";

describe("core messages", () => {
  it("switches remaining UI, statistics, exports and validation messages live", async () => {
    const originalGetLocale = runtimeGetLocale;
    overwriteGetLocale(getLocale);
    try {
      for (const locale of ["en", "zh-TW", "en"] as const) {
        applyLanguagePreference(locale);
        const chinese = locale === "zh-TW";
        const catalog = chinese ? zh : en;
        expect(m.share_network_discard_title()).toBe(
          catalog.share_network_discard_title,
        );
        expect(m.unsaved_changes_title()).toBe(catalog.unsaved_changes_title);
        expect(m.unsaved_changes_description()).toBe(
          catalog.unsaved_changes_description,
        );
        expect(m.unsaved_changes_keep_editing()).toBe(
          catalog.unsaved_changes_keep_editing,
        );
        expect(m.unsaved_changes_discard()).toBe(
          catalog.unsaved_changes_discard,
        );
        for (const permission of [m.board_read_only(), m.board_can_edit()]) {
          expect(m.share_invitation_settings({ permission })).toBe(
            `${chinese ? "邀請設定・" : "Invitation settings · "}${permission}`,
          );
        }
        expect(m.share_device_id_verified()).toBe(
          catalog.share_device_id_verified,
        );
        expect(m.share_owner_sync_no_editors()).toBe(
          catalog.share_owner_sync_no_editors,
        );
        for (const [synced, failed] of [
          [0, 1],
          [1, 1],
          [2, 2],
          [1234, 2],
        ]) {
          expect(m.share_owner_sync_partial({ synced, failed })).toBe(
            chinese
              ? `已與 ${formatNumber(synced)} 個裝置同步；${formatNumber(failed)} 個裝置無法同步。請保持雙方的 Cardbe 開啟後再試一次。`
              : `Synced with ${formatNumber(synced)}; ${formatNumber(failed)} ${failed === 1 ? "device" : "devices"} could not be synced. Keep both apps open and try again.`,
          );
        }
        expect(m.editor_bold()).toBe(catalog.editor_bold);
        expect(m.editor_inline_code_help()).toBe(
          catalog.editor_inline_code_help,
        );
        expect(m.editor_settings()).toBe(catalog.editor_settings);
        expect(m.editor_reference_card()).toBe(catalog.editor_reference_card);
        expect(m.editor_link_required()).toBe(catalog.editor_link_required);
        expect(m.ui_access_restored_conflict()).toBe(
          catalog.ui_access_restored_conflict,
        );
        expect(m.task_move_board()).toBe(
          chinese ? "移至其他看板" : "Move to Board",
        );
        expect(m.export_document_title()).toBe(
          chinese ? "行事曆匯出" : "Calendar export",
        );
        expect(m.export_month_label({ month: formatNumber(2) })).toBe(
          chinese ? "第 2 個月" : "Month 2",
        );
        expect(m.backup_restore_confirm()).toBe(
          chinese ? zh.backup_restore_confirm : en.backup_restore_confirm,
        );
        expect(m.ui_what_needs_doing()).toBe(
          chinese ? zh.ui_what_needs_doing : en.ui_what_needs_doing,
        );
        expect(m.common_search()).toBe(chinese ? "搜尋" : "Search");
        expect([
          m.task_new(),
          m.ui_new_note(),
          m.tray_enable_shortcuts(),
          m.tray_show(),
          m.tray_quit(),
        ]).toEqual(
          chinese
            ? [
                "新增任務",
                "新增筆記",
                "啟用全域快捷鍵",
                "顯示 Cardbe",
                "結束 Cardbe",
              ]
            : [
                "New Task",
                "New Note",
                "Enable global shortcuts",
                "Show Cardbe",
                "Quit Cardbe",
              ],
        );
        expect(m.notification_expired_title({ board: "{board}" })).toBe(
          chinese ? "任務已逾期 — {board}" : "Task Expired — {board}",
        );
        expect(m.notification_expired_body({ task: "{task}" })).toBe(
          chinese
            ? "任務「{task}」已超過截止時間"
            : 'Task "{task}" has passed its due date',
        );
        expect(create_card_reference("", "task_42")).toBe(
          chinese ? "[[無標題卡片|task_42]]" : "[[Untitled card|task_42]]",
        );
        expect(m.diagnostics_zip_archive()).toBe(
          chinese ? "ZIP 壓縮檔" : "ZIP archive",
        );
        expect(m.share_access_request_error()).toBe(
          chinese ? "無法申請存取權限" : "Couldn't request access",
        );
        expect(updateCheckErrorMessage({ code: "UPDATE_REQUEST_FAILED" })).toBe(
          chinese ? zh.update_network_error : en.update_network_error,
        );
        expect(
          updateCheckErrorMessage({ code: "UPDATE_RELEASE_DATA_INVALID" }),
        ).toBe(
          chinese ? zh.update_release_data_error : en.update_release_data_error,
        );
        expect(updateCheckErrorMessage("GitHub returned 403 Forbidden")).toBe(
          chinese ? zh.error_internal : en.error_internal,
        );
        expect(() => parse_requested_share_link("invalid")).toThrow(
          chinese ? zh.share_link_invalid : en.share_link_invalid,
        );
        expect(() => build_share_snapshot([], [], [], "")).toThrow(
          chinese ? zh.share_name_required : en.share_name_required,
        );
        await expect(parse_portable_task("invalid")).rejects.toThrow(
          chinese ? zh.task_transfer_unsupported : en.task_transfer_unsupported,
        );
        await expect(parse_portable_task("cardbe-task:v1:!")).rejects.toThrow(
          chinese ? zh.task_transfer_invalid : en.task_transfer_invalid,
        );
        for (const count of [0, 1, 1234]) {
          const formatted = formatNumber(count);
          const noun = count === 1 ? "task" : "tasks";
          expect(m.statistics_year_summary({ count, year: "2026" })).toBe(
            chinese
              ? `2026 年封存了 ${formatted} 個任務`
              : `${formatted} archived ${noun} in 2026`,
          );
          const date = formatDate(new Date(2026, 9, 4), { dateStyle: "full" });
          expect(m.statistics_day_summary({ count, date })).toBe(
            chinese
              ? `${date}：封存了 ${formatted} 個任務`
              : `${date}: ${formatted} archived ${noun}`,
          );
        }
      }
    } finally {
      overwriteGetLocale(originalGetLocale);
      applyLanguagePreference("system");
    }
  });
  it("keeps catalog keys aligned", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort());
    expect(
      Object.values(zh).some(
        (value) => typeof value === "string" && /\?{2,}|\uFFFD/.test(value),
      ),
    ).toBe(false);
  });
  it("uses locale-specific plurals", () => {
    expect(m.task_count({ count: 1 }, { locale: "en" })).toBe("1 task");
    expect(m.task_count({ count: 3 }, { locale: "en" })).toBe("3 tasks");
    expect(m.task_count({ count: 3 }, { locale: "zh-TW" })).toBe("3 個任務");
    expect(m.task_repeat_week_count({ count: 2 }, { locale: "en" })).toBe(
      "Every 2 weeks",
    );
    expect(m.task_repeat_week_count({ count: 2 }, { locale: "zh-TW" })).toBe(
      "每 2 週",
    );
  });
  it("preserves user content in complete messages", () => {
    expect(
      m.board_delete_named_confirm({ name: "我的看板" }, { locale: "en" }),
    ).toBe('Delete "我的看板"?');
    expect(
      m.task_delete_named_confirm({ name: "My task" }, { locale: "zh-TW" }),
    ).toBe("確定要刪除「My task」嗎？");
  });
  it("formats dates and numbers with the current preference", () => {
    const date = new Date(Date.UTC(2026, 9, 3, 10, 30));
    const options: Intl.DateTimeFormatOptions = {
      dateStyle: "long",
      timeZone: "UTC",
    };
    try {
      for (const locale of ["en", "zh-TW"] as const) {
        applyLanguagePreference(locale);
        expect(formatDate(date, options)).toBe(
          new Intl.DateTimeFormat(locale, options).format(date),
        );
        expect(formatDateTime(date)).toBe(
          new Intl.DateTimeFormat(locale, {
            dateStyle: "medium",
            timeStyle: "short",
          }).format(date),
        );
        expect(formatNumber(1234.5)).toBe(
          new Intl.NumberFormat(locale).format(1234.5),
        );
      }
    } finally {
      applyLanguagePreference("system");
    }
  });
  it("localizes calendar statistics, navigation, and export counts", () => {
    expect(m.calendar_due_summary({ count: 0 }, { locale: "en" })).toBe(
      "0 tasks with due dates",
    );
    expect(m.calendar_due_summary({ count: 1 }, { locale: "en" })).toBe(
      "1 task with due dates",
    );
    expect(m.calendar_month_summary({ count: 2 }, { locale: "en" })).toBe(
      "2 entries this month",
    );
    expect(m.calendar_month_summary({ count: 0 }, { locale: "zh-TW" })).toBe(
      "本月有 0 個項目",
    );
    expect(m.calendar_week_summary({ count: 1234 }, { locale: "zh-TW" })).toBe(
      "本週有 1,234 個項目",
    );
    expect(m.calendar_go_today({}, { locale: "zh-TW" })).toBe("跳至今天");
    expect(m.export_page_count({ count: 1 }, { locale: "en" })).toBe("1 page");
    expect(m.export_page_count({ count: 2 }, { locale: "en" })).toBe("2 pages");
    expect(m.export_month_count({ count: 12 }, { locale: "zh-TW" })).toBe(
      "共 12 個月",
    );
    expect(m.export_action({ format: "PDF" }, { locale: "zh-TW" })).toBe(
      "匯出 PDF",
    );
  });
  it("uses complete natural messages for focus and filtering", () => {
    expect(m.focus_today_clear({}, { locale: "zh-TW" })).toBe(
      "今天的待辦事項都處理好了。",
    );
    expect(m.focus_attention({ count: 1 }, { locale: "en" })).toBe(
      "1 task needs your attention.",
    );
    expect(m.focus_attention({ count: 2 }, { locale: "en" })).toBe(
      "2 tasks need your attention.",
    );
    expect(m.explorer_loaded({ count: 1234 }, { locale: "zh-TW" })).toBe(
      "已載入 1,234 個任務",
    );
    expect(
      m.explorer_actions_named({ name: "My task" }, { locale: "zh-TW" }),
    ).toBe("「My task」的操作");
  });
});
