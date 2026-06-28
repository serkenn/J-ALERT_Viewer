namespace JAlertReceiver;

/// <summary>
/// Thread-safe store of (a) the currently-in-force warning channels used by the
/// full-screen display and (b) a mailbox-style history of every received
/// telegram with server-side read/unread state. Per the configured policy, only
/// 警報 / 特別警報 trigger the full-screen alert; 注意報 are kept as a subdued
/// list. Raises <see cref="Changed"/> whenever the snapshot changes.
/// </summary>
public sealed class AlertState
{
    private const int HistoryCap = 500;     // items kept in memory
    private const int SnapshotInbox = 120;  // items sent to the browser

    private readonly object _gate = new();
    private readonly Dictionary<string, AlertChannel> _channels = new();
    private readonly List<InboxItem> _history = new();   // oldest first
    private readonly ReceiverStatus _receiver = new();
    private long _nextId = 1;

    public event Action<StateSnapshot>? Changed;

    public AlertState(string source) => _receiver.Source = source;

    /// <summary>Apply one decoded line: record it in the mailbox and update the live channels.</summary>
    public void Ingest(AlertChannel ch)
    {
        lock (_gate)
        {
            _receiver.LastLineMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            _receiver.TotalLines++;

            RecordHistoryLocked(ch);

            if (ch.Severity == Severity.None)
                _channels.Remove(ch.Key);       // all kinds cancelled -> drop channel
            else
                _channels[ch.Key] = ch;
        }
        Notify();
    }

    public void SetConnected(bool connected)
    {
        lock (_gate)
        {
            if (_receiver.Connected == connected) return;
            _receiver.Connected = connected;
        }
        Notify();
    }

    public bool MarkRead(long id, bool read)
    {
        lock (_gate)
        {
            var it = _history.FirstOrDefault(h => h.Id == id);
            if (it == null || it.Read == read) return false;
            it.Read = read;
        }
        Notify();
        return true;
    }

    public void MarkAllRead()
    {
        lock (_gate)
            foreach (var h in _history) h.Read = true;
        Notify();
    }

    public StateSnapshot Snapshot()
    {
        lock (_gate) return BuildLocked();
    }

    /// <summary>XML by mailbox item id, or by live-channel key.</summary>
    public string? XmlForId(long id)
    {
        lock (_gate) return _history.FirstOrDefault(h => h.Id == id)?.Xml;
    }

    public string? XmlForKey(string key)
    {
        lock (_gate) return _channels.TryGetValue(key, out var c) ? c.Xml : null;
    }

    private void Notify() => Changed?.Invoke(Snapshot());

    // ---- internals (caller holds _gate) ----

    private void RecordHistoryLocked(AlertChannel ch)
    {
        // Collapse retransmits: the plugin re-sends the same telegram every few
        // seconds. Skip if the most recent item for this area is identical.
        InboxItem? prev = null;
        for (int i = _history.Count - 1; i >= 0; i--)
            if (_history[i].HeadTitle == ch.HeadTitle) { prev = _history[i]; break; }

        var kinds = ch.Kinds.Select(k => k.Name).ToList();
        if (prev != null && prev.Severity == ch.Severity && prev.InfoType == ch.InfoType
            && prev.Headline == ch.Headline && prev.Kinds.SequenceEqual(kinds))
        {
            prev.RxTimeMs = ch.RxTimeMs;     // refresh timestamp, no new entry
            prev.PacketTime = ch.PacketTime;
            return;
        }

        var item = new InboxItem
        {
            Id = _nextId++,
            RxTimeMs = ch.RxTimeMs,
            PacketTime = ch.PacketTime,
            Severity = ch.Severity,
            SeverityLabel = LabelOf(ch.Severity),
            InfoType = ch.InfoType,
            Title = ch.Title,
            HeadTitle = ch.HeadTitle,
            AreaName = ch.AreaName,
            Kinds = kinds,
            Headline = ch.Headline,
            Read = false,
            Xml = ch.Xml,
        };
        _history.Add(item);
        if (_history.Count > HistoryCap) _history.RemoveRange(0, _history.Count - HistoryCap);
    }

    private static string LabelOf(Severity s) => s switch
    {
        Severity.Emergency => "特別警報",
        Severity.Warning => "警報",
        Severity.Advisory => "注意報",
        _ => "解除",
    };

    private StateSnapshot BuildLocked()
    {
        var all = _channels.Values
            .OrderByDescending(c => c.Severity)
            .ThenByDescending(c => c.RxTimeMs)
            .ToList();

        var alerts = all.Where(c => c.Severity >= Severity.Warning).ToList();
        var advisories = all.Where(c => c.Severity == Severity.Advisory).ToList();
        Severity top = all.Count > 0 ? all.Max(c => c.Severity) : Severity.None;
        string mode = top >= Severity.Warning ? "alert"
                    : top == Severity.Advisory ? "advisory" : "standby";

        var inbox = new List<InboxItem>(SnapshotInbox);
        for (int i = _history.Count - 1; i >= 0 && inbox.Count < SnapshotInbox; i--)
        {
            var h = _history[i];
            inbox.Add(new InboxItem   // copy without XML
            {
                Id = h.Id, RxTimeMs = h.RxTimeMs, PacketTime = h.PacketTime,
                Severity = h.Severity, SeverityLabel = h.SeverityLabel, InfoType = h.InfoType,
                Title = h.Title, HeadTitle = h.HeadTitle, AreaName = h.AreaName,
                Kinds = h.Kinds, Headline = h.Headline, Read = h.Read,
            });
        }

        return new StateSnapshot
        {
            Mode = mode,
            TopSeverity = top,
            Primary = alerts.FirstOrDefault(),
            Alerts = alerts,
            Advisories = advisories,
            Inbox = inbox,
            Unread = _history.Count(h => !h.Read),
            ServerTimeMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            Receiver = new ReceiverStatus
            {
                Connected = _receiver.Connected,
                Source = _receiver.Source,
                LastLineMs = _receiver.LastLineMs,
                TotalLines = _receiver.TotalLines,
            },
        };
    }
}
