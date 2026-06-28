using System.Text.Json.Serialization;

namespace JAlertReceiver;

/// <summary>Warning severity, ordered. Higher = more severe.</summary>
public enum Severity
{
    None = 0,
    Advisory = 1,   // 注意報
    Warning = 2,    // 警報
    Emergency = 3,  // 特別警報
}

/// <summary>One warning kind that is currently in force for an area.</summary>
public sealed class AlertKind
{
    public string Name { get; set; } = "";   // e.g. 大雨警報
    public string Status { get; set; } = "";  // 発表 / 継続 / 解除
    public Severity Severity { get; set; }
}

/// <summary>
/// The state of one weather-warning "channel", keyed by the prefectural
/// forecast area (Head/Title, e.g. "十勝地方気象警報・注意報"). A newer report
/// for the same channel supersedes the previous one.
/// </summary>
public sealed class AlertChannel
{
    public string Key { get; set; } = "";        // head_title
    public string Title { get; set; } = "";       // Control/Title
    public string HeadTitle { get; set; } = "";   // Head/Title
    public string AreaName { get; set; } = "";    // 府県予報区 area name
    public string InfoType { get; set; } = "";    // 発表 / 更新 / 取消
    public string Headline { get; set; } = "";
    public string ReportTime { get; set; } = "";  // ISO-8601 (JST)
    public string PacketTime { get; set; } = "";  // 17-digit
    public long RxTimeMs { get; set; }
    public Severity Severity { get; set; }
    public List<AlertKind> Kinds { get; set; } = new();  // active kinds only
    public List<string> Areas { get; set; } = new();     // sub-area names in force

    [JsonIgnore] public string Xml { get; set; } = "";
}

/// <summary>One received telegram as it appears in the mailbox (read/unread).</summary>
public sealed class InboxItem
{
    public long Id { get; set; }
    public long RxTimeMs { get; set; }
    public string PacketTime { get; set; } = "";
    public Severity Severity { get; set; }
    public string SeverityLabel { get; set; } = "";  // 特別警報/警報/注意報/解除
    public string InfoType { get; set; } = "";        // 発表/更新/取消
    public string Title { get; set; } = "";
    public string HeadTitle { get; set; } = "";
    public string AreaName { get; set; } = "";
    public List<string> Kinds { get; set; } = new();  // kind names
    public string Headline { get; set; } = "";
    public bool Read { get; set; }

    [JsonIgnore] public string Xml { get; set; } = "";
}

/// <summary>Snapshot pushed to the browser on every change.</summary>
public sealed class StateSnapshot
{
    public string Mode { get; set; } = "standby";   // standby | advisory | alert
    public Severity TopSeverity { get; set; }
    public AlertChannel? Primary { get; set; }       // the alert shown full-screen
    public List<AlertChannel> Alerts { get; set; } = new();      // 警報・特別警報
    public List<AlertChannel> Advisories { get; set; } = new();  // 注意報
    public List<InboxItem> Inbox { get; set; } = new();          // recent received (newest first)
    public int Unread { get; set; }
    public long ServerTimeMs { get; set; }
    public ReceiverStatus Receiver { get; set; } = new();
}

/// <summary>Health of the upstream TCP link to the SDR# plugin.</summary>
public sealed class ReceiverStatus
{
    public bool Connected { get; set; }
    public string Source { get; set; } = "";
    public long LastLineMs { get; set; }
    public long TotalLines { get; set; }
}
