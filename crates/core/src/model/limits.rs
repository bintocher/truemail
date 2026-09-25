//! Настраиваемые пределы и сроки программы.
//!
//! До этого раздела каждое такое число было записано прямо в коде, а половина
//! из них - ещё и вторым числом в интерфейсе. Копии расходились: ядро
//! разрешало закрепить 20 писем, а список показывал место под 50, и отказ
//! никто не объяснял. Здесь у каждого предела ровно одно описание: имя
//! настройки, значение по первому запуску, границы и человеческая подпись.
//! Ядро читает рабочее значение из настроек, интерфейс получает перечень этим
//! же описанием и своих чисел не держит.

use serde::Serialize;

/// Раздел настроек. Поля разложены по смыслу: сорок полей одним списком
/// пользователь не читает.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct LimitSection {
    pub id: &'static str,
    /// Ключ подписи в общем каталоге локализации
    /// (apps/desktop/ui/locales). Подпись хранится там, а не строкой здесь:
    /// каталог один на ядро и интерфейс, и перевод не расходится между ними.
    pub title_key: &'static str,
    pub hint_key: &'static str,
}

/// Описание одного предела: что настраивается, в каких границах и на что
/// влияет. Подпись пишется человеческим языком - имя настройки пользователю
/// ничего не говорит.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct LimitSpec {
    /// Ключ в таблице настроек.
    pub key: &'static str,
    /// Раздел, в котором поле показывается.
    pub section: &'static str,
    /// Ключ подписи поля в каталоге локализации.
    pub title_key: &'static str,
    /// Ключ пояснения - на что влияет значение.
    pub hint_key: &'static str,
    /// Ключ единицы измерения для подписи рядом с полем.
    pub unit_key: &'static str,
    /// Значение при первом запуске - равно прежнему зашитому в код.
    pub default: i64,
    pub min: i64,
    pub max: i64,
}

/// Разделы в порядке показа.
pub const LIMIT_SECTIONS: &[LimitSection] = &[
    LimitSection {
        id: "messages",
        title_key: "limitSectionMessages",
        hint_key: "limitSectionMessagesDesc",
    },
    LimitSection {
        id: "rules",
        title_key: "limitSectionRules",
        hint_key: "limitSectionRulesDesc",
    },
    LimitSection {
        id: "sending",
        title_key: "limitSectionSending",
        hint_key: "limitSectionSendingDesc",
    },
    LimitSection {
        id: "out_of_office",
        title_key: "limitSectionOutOfOffice",
        hint_key: "limitSectionOutOfOfficeDesc",
    },
    LimitSection {
        id: "maintenance",
        title_key: "limitSectionMaintenance",
        hint_key: "limitSectionMaintenanceDesc",
    },
];

// Ключи настроек. Имя ключа отдельной константой: строка, набранная руками во
// втором месте, разойдётся с первым молча.
pub const LIMIT_PINNED_PER_ACCOUNT: &str = "limit_pinned_per_account";
pub const LIMIT_PINNED_VISIBLE: &str = "limit_pinned_visible";
pub const LIMIT_MESSAGE_FIRST_PAGE: &str = "limit_message_first_page";
pub const LIMIT_MESSAGE_PAGE: &str = "limit_message_page";
pub const LIMIT_SMART_MESSAGE_PAGE: &str = "limit_smart_message_page";
pub const LIMIT_MESSAGE_MEMORY: &str = "limit_message_memory";
pub const LIMIT_SYNC_FAILURES_BEFORE_TOAST: &str = "limit_sync_failures_before_toast";

pub const LIMIT_RULE_GROUPS: &str = "limit_rule_groups";
pub const LIMIT_GROUP_CONDITIONS: &str = "limit_group_conditions";
pub const LIMIT_RULE_ACTIONS: &str = "limit_rule_actions";
pub const LIMIT_QUICK_STEPS: &str = "limit_quick_steps";
pub const LIMIT_QUICK_STEP_MESSAGES: &str = "limit_quick_step_messages";
pub const LIMIT_MANUAL_RUN_MESSAGES: &str = "limit_manual_run_messages";
pub const LIMIT_MANUAL_RUN_BATCH: &str = "limit_manual_run_batch";

pub const LIMIT_UNDO_SEND_MIN: &str = "limit_undo_send_min";
pub const LIMIT_UNDO_SEND_MAX: &str = "limit_undo_send_max";
pub const LIMIT_UNDO_SEND_DEFAULT: &str = "limit_undo_send_default";
pub const LIMIT_RECIPIENT_ENTRIES: &str = "limit_recipient_entries";
pub const LIMIT_RECIPIENT_TOUCHES: &str = "limit_recipient_touches";
pub const LIMIT_RECIPIENT_SUGGESTIONS: &str = "limit_recipient_suggestions";

pub const LIMIT_OOF_SILENCE_DAYS: &str = "limit_oof_silence_days";
pub const LIMIT_OOF_MEMORY_DAYS: &str = "limit_oof_memory_days";
pub const LIMIT_OOF_MESSAGE_AGE_HOURS: &str = "limit_oof_message_age_hours";
pub const LIMIT_OOF_TEXT_CHARS: &str = "limit_oof_text_chars";
pub const LIMIT_OOF_PERIOD_DAYS: &str = "limit_oof_period_days";
pub const LIMIT_OOF_INTERNAL_DOMAINS: &str = "limit_oof_internal_domains";

pub const LIMIT_DONE_TASK_DAYS: &str = "limit_done_task_days";
pub const LIMIT_IGNORED_CONVERSATIONS: &str = "limit_ignored_conversations";
pub const LIMIT_CONVERSATION_IDS: &str = "limit_conversation_ids";
pub const LIMIT_IGNORE_RETURN_WAIT_DAYS: &str = "limit_ignore_return_wait_days";
pub const LIMIT_SWEEP_MIN_DAYS: &str = "limit_sweep_min_days";
pub const LIMIT_SWEEP_MAX_DAYS: &str = "limit_sweep_max_days";
pub const LIMIT_PURGE_BATCH: &str = "limit_purge_batch";
pub const LIMIT_REQUEST_KEY_DAYS: &str = "limit_request_key_days";
pub const LIMIT_RECIPIENT_OWN_SEND_DAYS: &str = "limit_recipient_own_send_days";
pub const LIMIT_MESSAGE_TRAITS_DAYS: &str = "limit_message_traits_days";
pub const LIMIT_OPERATION_ATTEMPTS: &str = "limit_operation_attempts";
pub const LIMIT_BACKGROUND_SYNC_MINUTES: &str = "limit_background_sync_minutes";
pub const LIMIT_GMAIL_POLL_SECONDS: &str = "limit_gmail_poll_seconds";
pub const LIMIT_SNOOZE_RELEASE_SECONDS: &str = "limit_snooze_release_seconds";
pub const LIMIT_BACKFILL_PAGE: &str = "limit_backfill_page";
pub const LIMIT_BODY_PREFETCH_MESSAGES: &str = "limit_body_prefetch_messages";
pub const LIMIT_BODY_PREFETCH_SIZE_MB: &str = "limit_body_prefetch_size_mb";

pub const LIMIT_HISTORY_PAGE: &str = "limit_history_page";
pub const LIMIT_RANK_FRESH_DAYS: &str = "limit_rank_fresh_days";
pub const LIMIT_RANK_RECENT_DAYS: &str = "limit_rank_recent_days";
pub const LIMIT_RANK_OLD_DAYS: &str = "limit_rank_old_days";

pub const LIMIT_STAGE_BATCH: &str = "limit_stage_batch";
pub const LIMIT_STAGE_SNAPSHOT_HOURS: &str = "limit_stage_snapshot_hours";
pub const LIMIT_SWEEP_WAIT_SECONDS: &str = "limit_sweep_wait_seconds";
pub const LIMIT_SWEEP_MAX_WAITS: &str = "limit_sweep_max_waits";
pub const LIMIT_SWEEP_FULL_PASS_HOURS: &str = "limit_sweep_full_pass_hours";
pub const LIMIT_REMINDER_CHECK_SECONDS: &str = "limit_reminder_check_seconds";
pub const LIMIT_UPDATE_CHECK_HOURS: &str = "limit_update_check_hours";
pub const LIMIT_ACTIVITY_LOG_ENTRIES: &str = "limit_activity_log_entries";
pub const LIMIT_ACTIVITY_LOG_VISIBLE: &str = "limit_activity_log_visible";

/// Перечень настраиваемых пределов. Порядок - порядок показа внутри раздела.
pub const LIMITS: &[LimitSpec] = &[
    // --- Письма и список ---
    LimitSpec {
        key: LIMIT_PINNED_PER_ACCOUNT,
        section: "messages",
        title_key: "limitPinnedPerAccount",
        hint_key: "limitPinnedPerAccountDesc",
        unit_key: "limitUnitMessages",
        default: 20,
        min: 1,
        max: 1000,
    },
    LimitSpec {
        key: LIMIT_PINNED_VISIBLE,
        section: "messages",
        title_key: "limitPinnedVisible",
        hint_key: "limitPinnedVisibleDesc",
        unit_key: "limitUnitMessages",
        default: 50,
        min: 1,
        max: 1000,
    },
    LimitSpec {
        key: LIMIT_MESSAGE_FIRST_PAGE,
        section: "messages",
        title_key: "limitMessageFirstPage",
        hint_key: "limitMessageFirstPageDesc",
        unit_key: "limitUnitMessages",
        default: 100,
        min: 10,
        max: 2000,
    },
    LimitSpec {
        key: LIMIT_MESSAGE_PAGE,
        section: "messages",
        title_key: "limitMessagePage",
        hint_key: "limitMessagePageDesc",
        unit_key: "limitUnitMessages",
        default: 500,
        min: 10,
        max: 5000,
    },
    LimitSpec {
        key: LIMIT_SMART_MESSAGE_PAGE,
        section: "messages",
        title_key: "limitSmartMessagePage",
        hint_key: "limitSmartMessagePageDesc",
        unit_key: "limitUnitMessages",
        default: 500,
        min: 10,
        max: 5000,
    },
    LimitSpec {
        key: LIMIT_MESSAGE_MEMORY,
        section: "messages",
        title_key: "limitMessageMemory",
        hint_key: "limitMessageMemoryDesc",
        unit_key: "limitUnitMessages",
        default: 8000,
        min: 500,
        max: 100_000,
    },
    LimitSpec {
        key: LIMIT_BACKGROUND_SYNC_MINUTES,
        section: "messages",
        title_key: "limitBackgroundSyncMinutes",
        hint_key: "limitBackgroundSyncMinutesDesc",
        unit_key: "limitUnitMinutes",
        default: 5,
        min: 1,
        max: 1440,
    },
    LimitSpec {
        key: LIMIT_SYNC_FAILURES_BEFORE_TOAST,
        section: "messages",
        title_key: "limitSyncFailuresBeforeToast",
        hint_key: "limitSyncFailuresBeforeToastDesc",
        unit_key: "limitUnitChecks",
        default: 3,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_GMAIL_POLL_SECONDS,
        section: "messages",
        title_key: "limitGmailPollSeconds",
        hint_key: "limitGmailPollSecondsDesc",
        unit_key: "limitUnitSeconds",
        default: 25,
        min: 5,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_SNOOZE_RELEASE_SECONDS,
        section: "messages",
        title_key: "limitSnoozeReleaseSeconds",
        hint_key: "limitSnoozeReleaseSecondsDesc",
        unit_key: "limitUnitSeconds",
        default: 30,
        min: 5,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_BACKFILL_PAGE,
        section: "messages",
        title_key: "limitBackfillPage",
        hint_key: "limitBackfillPageDesc",
        unit_key: "limitUnitMessages",
        default: 15,
        min: 1,
        max: 500,
    },
    LimitSpec {
        key: LIMIT_BODY_PREFETCH_MESSAGES,
        section: "messages",
        title_key: "limitBodyPrefetchMessages",
        hint_key: "limitBodyPrefetchMessagesDesc",
        unit_key: "limitUnitMessages",
        default: 50,
        min: 1,
        max: 1000,
    },
    LimitSpec {
        key: LIMIT_BODY_PREFETCH_SIZE_MB,
        section: "messages",
        title_key: "limitBodyPrefetchSizeMb",
        hint_key: "limitBodyPrefetchSizeMbDesc",
        unit_key: "limitUnitMegabytes",
        default: 5,
        min: 1,
        max: 100,
    },
    // --- Правила и действия ---
    LimitSpec {
        key: LIMIT_RULE_GROUPS,
        section: "rules",
        title_key: "limitRuleGroups",
        hint_key: "limitRuleGroupsDesc",
        unit_key: "limitUnitGroups",
        default: 10,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_GROUP_CONDITIONS,
        section: "rules",
        title_key: "limitGroupConditions",
        hint_key: "limitGroupConditionsDesc",
        unit_key: "limitUnitConditions",
        default: 10,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_RULE_ACTIONS,
        section: "rules",
        title_key: "limitRuleActions",
        hint_key: "limitRuleActionsDesc",
        unit_key: "limitUnitActions",
        default: 10,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_QUICK_STEPS,
        section: "rules",
        title_key: "limitQuickSteps",
        hint_key: "limitQuickStepsDesc",
        unit_key: "limitUnitActions",
        default: 20,
        min: 1,
        max: 200,
    },
    LimitSpec {
        key: LIMIT_QUICK_STEP_MESSAGES,
        section: "rules",
        title_key: "limitQuickStepMessages",
        hint_key: "limitQuickStepMessagesDesc",
        unit_key: "limitUnitMessages",
        default: 500,
        min: 1,
        max: 10_000,
    },
    LimitSpec {
        key: LIMIT_MANUAL_RUN_MESSAGES,
        section: "rules",
        title_key: "limitManualRunMessages",
        hint_key: "limitManualRunMessagesDesc",
        unit_key: "limitUnitMessages",
        default: 5000,
        min: 100,
        max: 200_000,
    },
    LimitSpec {
        key: LIMIT_MANUAL_RUN_BATCH,
        section: "rules",
        title_key: "limitManualRunBatch",
        hint_key: "limitManualRunBatchDesc",
        unit_key: "limitUnitMessages",
        default: 500,
        min: 10,
        max: 10_000,
    },
    // --- Отправка ---
    LimitSpec {
        key: LIMIT_UNDO_SEND_DEFAULT,
        section: "sending",
        title_key: "limitUndoSendDefault",
        hint_key: "limitUndoSendDefaultDesc",
        unit_key: "limitUnitSeconds",
        default: 5,
        min: 0,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_UNDO_SEND_MIN,
        section: "sending",
        title_key: "limitUndoSendMin",
        hint_key: "limitUndoSendMinDesc",
        unit_key: "limitUnitSeconds",
        default: 0,
        min: 0,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_UNDO_SEND_MAX,
        section: "sending",
        title_key: "limitUndoSendMax",
        hint_key: "limitUndoSendMaxDesc",
        unit_key: "limitUnitSeconds",
        default: 60,
        min: 1,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_RECIPIENT_ENTRIES,
        section: "sending",
        title_key: "limitRecipientEntries",
        hint_key: "limitRecipientEntriesDesc",
        unit_key: "limitUnitEntries",
        default: 2000,
        min: 10,
        max: 100_000,
    },
    LimitSpec {
        key: LIMIT_RECIPIENT_TOUCHES,
        section: "sending",
        title_key: "limitRecipientTouches",
        hint_key: "limitRecipientTouchesDesc",
        unit_key: "limitUnitTouches",
        default: 50,
        min: 1,
        max: 1000,
    },
    LimitSpec {
        key: LIMIT_RECIPIENT_SUGGESTIONS,
        section: "sending",
        title_key: "limitRecipientSuggestions",
        hint_key: "limitRecipientSuggestionsDesc",
        unit_key: "limitUnitAddresses",
        default: 8,
        min: 1,
        max: 50,
    },
    LimitSpec {
        key: LIMIT_HISTORY_PAGE,
        section: "sending",
        title_key: "limitHistoryPage",
        hint_key: "limitHistoryPageDesc",
        unit_key: "limitUnitEntries",
        default: 100,
        min: 10,
        max: 1000,
    },
    // Пороги свежести идут от свежего к давнему: обращение, попавшее в первый
    // порог, весит больше, чем попавшее во второй. Сами веса настройкой не
    // стали - они задают не предел, а само правило сравнения.
    LimitSpec {
        key: LIMIT_RANK_FRESH_DAYS,
        section: "sending",
        title_key: "limitRankFreshDays",
        hint_key: "limitRankFreshDaysDesc",
        unit_key: "limitUnitDays",
        default: 30,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_RANK_RECENT_DAYS,
        section: "sending",
        title_key: "limitRankRecentDays",
        hint_key: "limitRankRecentDaysDesc",
        unit_key: "limitUnitDays",
        default: 90,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_RANK_OLD_DAYS,
        section: "sending",
        title_key: "limitRankOldDays",
        hint_key: "limitRankOldDaysDesc",
        unit_key: "limitUnitDays",
        default: 365,
        min: 1,
        max: 3650,
    },
    // --- Автоответ ---
    LimitSpec {
        key: LIMIT_OOF_SILENCE_DAYS,
        section: "out_of_office",
        title_key: "limitOofSilenceDays",
        hint_key: "limitOofSilenceDaysDesc",
        unit_key: "limitUnitDays",
        default: 7,
        min: 1,
        max: 365,
    },
    LimitSpec {
        key: LIMIT_OOF_MEMORY_DAYS,
        section: "out_of_office",
        title_key: "limitOofMemoryDays",
        hint_key: "limitOofMemoryDaysDesc",
        unit_key: "limitUnitDays",
        default: 60,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_OOF_MESSAGE_AGE_HOURS,
        section: "out_of_office",
        title_key: "limitOofMessageAgeHours",
        hint_key: "limitOofMessageAgeHoursDesc",
        unit_key: "limitUnitHours",
        default: 24,
        min: 1,
        max: 8760,
    },
    LimitSpec {
        key: LIMIT_OOF_TEXT_CHARS,
        section: "out_of_office",
        title_key: "limitOofTextChars",
        hint_key: "limitOofTextCharsDesc",
        unit_key: "limitUnitChars",
        default: 10_000,
        min: 100,
        max: 1_000_000,
    },
    LimitSpec {
        key: LIMIT_OOF_PERIOD_DAYS,
        section: "out_of_office",
        title_key: "limitOofPeriodDays",
        hint_key: "limitOofPeriodDaysDesc",
        unit_key: "limitUnitDays",
        default: 366,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_OOF_INTERNAL_DOMAINS,
        section: "out_of_office",
        title_key: "limitOofInternalDomains",
        hint_key: "limitOofInternalDomainsDesc",
        unit_key: "limitUnitDomains",
        default: 20,
        min: 1,
        max: 500,
    },
    // --- Обслуживание ---
    LimitSpec {
        key: LIMIT_DONE_TASK_DAYS,
        section: "maintenance",
        title_key: "limitDoneTaskDays",
        hint_key: "limitDoneTaskDaysDesc",
        unit_key: "limitUnitDays",
        default: 30,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_IGNORED_CONVERSATIONS,
        section: "maintenance",
        title_key: "limitIgnoredConversations",
        hint_key: "limitIgnoredConversationsDesc",
        unit_key: "limitUnitConversations",
        default: 1000,
        min: 1,
        max: 100_000,
    },
    LimitSpec {
        key: LIMIT_CONVERSATION_IDS,
        section: "maintenance",
        title_key: "limitConversationIds",
        hint_key: "limitConversationIdsDesc",
        unit_key: "limitUnitStrings",
        default: 1000,
        min: 10,
        max: 10_000,
    },
    LimitSpec {
        key: LIMIT_IGNORE_RETURN_WAIT_DAYS,
        section: "maintenance",
        title_key: "limitIgnoreReturnWaitDays",
        hint_key: "limitIgnoreReturnWaitDaysDesc",
        unit_key: "limitUnitDays",
        default: 7,
        min: 1,
        max: 365,
    },
    LimitSpec {
        key: LIMIT_SWEEP_MIN_DAYS,
        section: "maintenance",
        title_key: "limitSweepMinDays",
        hint_key: "limitSweepMinDaysDesc",
        unit_key: "limitUnitDays",
        default: 1,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_SWEEP_MAX_DAYS,
        section: "maintenance",
        title_key: "limitSweepMaxDays",
        hint_key: "limitSweepMaxDaysDesc",
        unit_key: "limitUnitDays",
        default: 3650,
        min: 1,
        max: 36_500,
    },
    LimitSpec {
        key: LIMIT_PURGE_BATCH,
        section: "maintenance",
        title_key: "limitPurgeBatch",
        hint_key: "limitPurgeBatchDesc",
        unit_key: "limitUnitEntries",
        default: 500,
        min: 10,
        max: 10_000,
    },
    LimitSpec {
        key: LIMIT_REQUEST_KEY_DAYS,
        section: "maintenance",
        title_key: "limitRequestKeyDays",
        hint_key: "limitRequestKeyDaysDesc",
        unit_key: "limitUnitDays",
        default: 30,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_RECIPIENT_OWN_SEND_DAYS,
        section: "maintenance",
        title_key: "limitRecipientOwnSendDays",
        hint_key: "limitRecipientOwnSendDaysDesc",
        unit_key: "limitUnitDays",
        default: 30,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_OPERATION_ATTEMPTS,
        section: "maintenance",
        title_key: "limitOperationAttempts",
        hint_key: "limitOperationAttemptsDesc",
        unit_key: "limitUnitAttempts",
        default: 8,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_MESSAGE_TRAITS_DAYS,
        section: "maintenance",
        title_key: "limitMessageTraitsDays",
        hint_key: "limitMessageTraitsDaysDesc",
        unit_key: "limitUnitDays",
        default: 30,
        min: 1,
        max: 3650,
    },
    LimitSpec {
        key: LIMIT_STAGE_BATCH,
        section: "maintenance",
        title_key: "limitStageBatch",
        hint_key: "limitStageBatchDesc",
        unit_key: "limitUnitMessages",
        default: 500,
        min: 10,
        max: 10_000,
    },
    LimitSpec {
        key: LIMIT_STAGE_SNAPSHOT_HOURS,
        section: "maintenance",
        title_key: "limitStageSnapshotHours",
        hint_key: "limitStageSnapshotHoursDesc",
        unit_key: "limitUnitHours",
        default: 24,
        min: 1,
        max: 8760,
    },
    LimitSpec {
        key: LIMIT_SWEEP_WAIT_SECONDS,
        section: "maintenance",
        title_key: "limitSweepWaitSeconds",
        hint_key: "limitSweepWaitSecondsDesc",
        unit_key: "limitUnitSeconds",
        default: 60,
        min: 5,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_SWEEP_MAX_WAITS,
        section: "maintenance",
        title_key: "limitSweepMaxWaits",
        hint_key: "limitSweepMaxWaitsDesc",
        unit_key: "limitUnitWaits",
        default: 8,
        min: 1,
        max: 100,
    },
    LimitSpec {
        key: LIMIT_SWEEP_FULL_PASS_HOURS,
        section: "maintenance",
        title_key: "limitSweepFullPassHours",
        hint_key: "limitSweepFullPassHoursDesc",
        unit_key: "limitUnitHours",
        default: 24,
        min: 1,
        max: 8760,
    },
    LimitSpec {
        key: LIMIT_REMINDER_CHECK_SECONDS,
        section: "maintenance",
        title_key: "limitReminderCheckSeconds",
        hint_key: "limitReminderCheckSecondsDesc",
        unit_key: "limitUnitSeconds",
        default: 60,
        min: 5,
        max: 3600,
    },
    LimitSpec {
        key: LIMIT_UPDATE_CHECK_HOURS,
        section: "maintenance",
        title_key: "limitUpdateCheckHours",
        hint_key: "limitUpdateCheckHoursDesc",
        unit_key: "limitUnitHours",
        default: 6,
        min: 1,
        max: 8760,
    },
    // Журнал событий в строке статуса живёт только в памяти окна; ядро его не
    // читает, предел нужен интерфейсу (specs/status-activity-log.md).
    LimitSpec {
        key: LIMIT_ACTIVITY_LOG_ENTRIES,
        section: "maintenance",
        title_key: "limitActivityLogEntries",
        hint_key: "limitActivityLogEntriesDesc",
        unit_key: "limitUnitEntries",
        default: 20,
        min: 1,
        max: 500,
    },
    LimitSpec {
        key: LIMIT_ACTIVITY_LOG_VISIBLE,
        section: "maintenance",
        title_key: "limitActivityLogVisible",
        hint_key: "limitActivityLogVisibleDesc",
        unit_key: "limitUnitRows",
        default: 10,
        min: 1,
        max: 100,
    },
];

/// Связанные пределы: значения перечисленных ключей идут по неубыванию.
///
/// Поодиночке каждый из них остаётся в своих границах, а вместе расходятся:
/// наименьшее окно отмены, поднятое выше значения первого запуска, оставляет
/// обычную отправку без пригодной длительности - выбранное по умолчанию окно
/// отклоняется при приёме письма, и отправка перестаёт работать целиком.
pub const LIMIT_ORDERS: &[&[&str]] = &[&[
    LIMIT_UNDO_SEND_MIN,
    LIMIT_UNDO_SEND_DEFAULT,
    LIMIT_UNDO_SEND_MAX,
]];

/// Цепочки пределов, идущих строго по возрастанию. Пороги свежести обращения
/// образуют шкалу "свежее - недавнее - давнее", и сравнение идёт по порядку:
/// порог, догнавший предыдущий, недостижим - вес за ним не выдаётся никогда, а
/// подсказка получателей молча перестаёт различать свежие и недавние
/// обращения. От неубывающих цепочек это отделено намеренно: там равные
/// значения осмысленны, здесь равенство ломает шкалу.
pub const LIMIT_STRICT_ORDERS: &[&[&str]] = &[&[
    LIMIT_RANK_FRESH_DAYS,
    LIMIT_RANK_RECENT_DAYS,
    LIMIT_RANK_OLD_DAYS,
]];

/// Снимок рабочих значений пределов.
///
/// Чистые функции проверки (правила, автоответ, автоочистка) базы не видят и
/// видеть не должны: их решение проверяется без неё. Снимок берётся один раз
/// на вызов и передаётся в них ссылкой - так значение остаётся настройкой, а
/// проверка остаётся чистой.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitSet {
    values: std::collections::HashMap<&'static str, i64>,
}

impl LimitSet {
    /// Снимок из значений первого запуска. Им пользуются проверки и база,
    /// открытая в обход миграций.
    pub fn defaults() -> Self {
        Self {
            values: LIMITS.iter().map(|spec| (spec.key, spec.default)).collect(),
        }
    }

    /// Снимок из уже прочитанных настроек. Неизвестное и выходящее за границы
    /// значение заменяется значением первого запуска: одна испорченная строка
    /// в базе не должна закрывать доступ к почте.
    pub fn from_settings<F>(read: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut set = Self {
            values: LIMITS
                .iter()
                .map(|spec| {
                    let value = read(spec.key)
                        .and_then(|raw| raw.trim().parse::<i64>().ok())
                        .filter(|value| (spec.min..=spec.max).contains(value))
                        .unwrap_or(spec.default);
                    (spec.key, value)
                })
                .collect(),
        };
        // Собственные границы не видят связей между пределами, а в настройки
        // несогласованный набор попадает и мимо записи - перенесённой базой или
        // правкой руками. Расходящаяся цепочка целиком возвращается к значениям
        // первого запуска: они согласованы по построению, а починить одно звено
        // означало бы угадывать, какое из значений пользователь считал верным.
        for (chains, strict) in [(LIMIT_ORDERS, false), (LIMIT_STRICT_ORDERS, true)] {
            for chain in chains {
                if chain.windows(2).any(|pair| {
                    let (lower, upper) = (set.get(pair[0]), set.get(pair[1]));
                    lower > upper || (strict && lower == upper)
                }) {
                    for key in *chain {
                        if let Some(spec) = limit_spec(key) {
                            set.set(spec.key, spec.default);
                        }
                    }
                }
            }
        }
        set
    }

    pub fn get(&self, key: &str) -> i64 {
        self.values
            .get(key)
            .copied()
            .unwrap_or_else(|| limit_default(key))
    }

    /// То же значение числом нужного места. Отрицательных пределов нет:
    /// границы описания их не пропускают.
    pub fn count(&self, key: &str) -> usize {
        self.get(key).max(0) as usize
    }

    /// Заменить одно значение. Используется записью настройки и проверками.
    pub fn set(&mut self, key: &'static str, value: i64) {
        self.values.insert(key, value);
    }

    /// Перечень для интерфейса: описание вместе с рабочим значением.
    pub fn view(&self) -> Vec<LimitValue> {
        LIMITS
            .iter()
            .map(|spec| LimitValue {
                spec: *spec,
                value: self.get(spec.key),
            })
            .collect()
    }
}

impl Default for LimitSet {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Предел вместе с рабочим значением - то, что уходит в интерфейс.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LimitValue {
    #[serde(flatten)]
    pub spec: LimitSpec,
    pub value: i64,
}

/// Описание предела по ключу.
pub fn limit_spec(key: &str) -> Option<&'static LimitSpec> {
    LIMITS.iter().find(|spec| spec.key == key)
}

/// Значение по первому запуску. Ключ здесь всегда известен: его дают
/// константы этого же модуля, и неизвестное имя означает ошибку в коде.
pub fn limit_default(key: &str) -> i64 {
    limit_spec(key).map_or(0, |spec| spec.default)
}

/// Проверить значение предела. Отказ объясняется, а не глотается: молча
/// поправленное значение пользователь принял бы за принятое.
pub fn validate_limit(key: &str, value: i64) -> Result<i64, String> {
    let Some(spec) = limit_spec(key) else {
        return Err(format!("неизвестная настройка {key}"));
    };
    if value < spec.min || value > spec.max {
        // Подпись берётся из русского каталога: отказ объясняется человеку
        // названием настройки, а не именем ключа.
        let text = crate::i18n::I18n::new("ru");
        return Err(format!(
            "\"{}\": допустимы значения от {} до {} ({}), получено {}",
            text.t(spec.title_key),
            spec.min,
            spec.max,
            text.t(spec.unit_key),
            value
        ));
    }
    Ok(value)
}

/// Проверить значение вместе с соседями по цепочке. Каждое из связанных
/// значений остаётся в собственных границах, а вместе они расходятся, и
/// поведение, которое задаёт соседнее поле, становится недостижимым.
pub fn validate_limit_in_set(key: &str, value: i64, limits: &LimitSet) -> Result<i64, String> {
    let value = validate_limit(key, value)?;
    for (chains, strict) in [(LIMIT_ORDERS, false), (LIMIT_STRICT_ORDERS, true)] {
        for chain in chains {
            let Some(place) = chain.iter().position(|item| *item == key) else {
                continue;
            };
            if let Some(lower) = place.checked_sub(1).map(|index| chain[index]) {
                let bound = limits.get(lower);
                if value < bound || (strict && value == bound) {
                    return Err(order_refusal(key, value, lower, bound, "больше"));
                }
            }
            if let Some(upper) = chain.get(place + 1) {
                let bound = limits.get(upper);
                if value > bound || (strict && value == bound) {
                    return Err(order_refusal(key, value, upper, bound, "меньше"));
                }
            }
        }
    }
    Ok(value)
}

/// Отказ по цепочке. Называет зависимое поле и его рабочее значение: "должно
/// быть больше" без имени соседа не говорит пользователю, что именно менять.
fn order_refusal(key: &str, value: i64, neighbour: &str, bound: i64, order: &str) -> String {
    let text = crate::i18n::I18n::new("ru");
    let title =
        |key: &str| limit_spec(key).map_or_else(|| key.to_owned(), |spec| text.t(spec.title_key));
    let unit = limit_spec(neighbour).map_or(String::new(), |spec| text.t(spec.unit_key));
    format!(
        "\"{}\": значение должно быть {} значения настройки \"{}\" ({} {}), получено {}",
        title(key),
        order,
        title(neighbour),
        bound,
        unit,
        value
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ключ, повторённый в двух описаниях, дал бы два поля на одну настройку:
    /// пользователь менял бы одно, а действовало бы другое.
    #[test]
    fn limit_keys_and_sections_are_consistent() {
        let mut keys: Vec<&str> = LIMITS.iter().map(|spec| spec.key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "повторяющийся ключ предела");
        for spec in LIMITS {
            assert!(
                LIMIT_SECTIONS.iter().any(|item| item.id == spec.section),
                "предел {} ссылается на несуществующий раздел {}",
                spec.key,
                spec.section
            );
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "значение по умолчанию предела {} вне собственных границ",
                spec.key
            );
            // Забытая подпись оставила бы поле без названия: каталог
            // возвращает сам ключ, и пользователь увидел бы limitPinnedVisible.
            let text = crate::i18n::I18n::new("ru");
            for key in [spec.title_key, spec.hint_key, spec.unit_key] {
                assert_ne!(text.t(key), key, "нет русской подписи для ключа {key}");
            }
        }
        // Значения первого запуска связанных пределов обязаны идти по
        // неубыванию: на них возвращается расходящаяся цепочка из настроек, и
        // несогласованный реестр чинить было бы нечем.
        for chain in LIMIT_ORDERS {
            for pair in chain.windows(2) {
                assert!(
                    limit_default(pair[0]) <= limit_default(pair[1]),
                    "значения первого запуска пределов {} и {} идут не по порядку",
                    pair[0],
                    pair[1]
                );
            }
        }
        // У строгой цепочки равенство тоже запрещено: порог, догнавший
        // предыдущий, недостижим, и пользователь не смог бы записать ни одно
        // значение, которое сосед не отклонит.
        for chain in LIMIT_STRICT_ORDERS {
            for pair in chain.windows(2) {
                assert!(
                    limit_default(pair[0]) < limit_default(pair[1]),
                    "значения первого запуска пределов {} и {} не образуют шкалу",
                    pair[0],
                    pair[1]
                );
            }
        }
    }
}
