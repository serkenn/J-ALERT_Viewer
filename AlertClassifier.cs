using System.Text.Json;
using System.Xml.Linq;

namespace JAlertReceiver;

/// <summary>
/// Turns one JSONL line from the plugin into an <see cref="AlertChannel"/>.
///
/// Severity is derived authoritatively from the inflated JMA XML: the
/// Body/Warning section whose <c>type</c> is the prefectural forecast area
/// (気象警報・注意報（府県予報区等）) lists each warning Kind with a Status of
/// 発表 / 継続 / 解除. A Kind is "in force" when its Status is not 解除/なし; its
/// severity comes from the Name suffix (特別警報 &gt; 警報 &gt; 注意報).
/// </summary>
public static class AlertClassifier
{
    /// <summary>Returns null for lines that carry no usable weather warning.</summary>
    public static AlertChannel? FromJsonLine(string line)
    {
        if (string.IsNullOrWhiteSpace(line)) return null;

        JsonElement root;
        try { root = JsonDocument.Parse(line).RootElement; }
        catch { return null; }

        // Only decoded JMA telegrams carry XML we can classify.
        if (!GetBool(root, "decoded")) return null;
        string xml = GetString(root, "xml");
        if (xml.Length == 0) return null;

        var ch = new AlertChannel
        {
            Title = GetString(root, "title"),
            HeadTitle = GetString(root, "head_title"),
            InfoType = GetString(root, "info_type"),
            Headline = GetString(root, "headline"),
            ReportTime = GetString(root, "report_time"),
            PacketTime = GetString(root, "packet_time"),
            RxTimeMs = GetLong(root, "rx_time_ms"),
            Xml = xml,
        };
        ch.Key = ch.HeadTitle.Length > 0 ? ch.HeadTitle
               : (ch.Title.Length > 0 ? ch.Title : ch.PacketTime);

        ClassifyFromXml(xml, ch);
        return ch;
    }

    private static void ClassifyFromXml(string xml, AlertChannel ch)
    {
        XDocument doc;
        try { doc = XDocument.Parse(xml); }
        catch { return; }

        // Prefer the prefectural-area Warning block; fall back to the first one.
        XElement? warning = doc.Descendants()
            .Where(e => e.Name.LocalName == "Warning")
            .FirstOrDefault(e => ((string?)e.Attribute("type"))?.Contains("府県予報区") == true);
        warning ??= doc.Descendants().FirstOrDefault(e => e.Name.LocalName == "Warning");
        if (warning == null) return;

        var kinds = new List<AlertKind>();
        var areas = new HashSet<string>();
        Severity top = Severity.None;

        foreach (XElement item in warning.Elements().Where(e => e.Name.LocalName == "Item"))
        {
            foreach (XElement kind in item.Elements().Where(e => e.Name.LocalName == "Kind"))
            {
                string name = Local(kind, "Name");
                string status = Local(kind, "Status");
                if (name.Length == 0) continue;
                if (status == "解除" || status == "なし") continue;  // not in force

                Severity sev = SeverityOf(name);
                if (sev == Severity.None) continue;
                kinds.Add(new AlertKind { Name = name, Status = status, Severity = sev });
                if (sev > top) top = sev;
            }
            string area = item.Elements().Where(e => e.Name.LocalName == "Area")
                              .Select(a => Local(a, "Name")).FirstOrDefault(s => s.Length > 0) ?? "";
            if (area.Length > 0) areas.Add(area);
        }

        // Collapse duplicate kinds (same name appears across several area blocks).
        ch.Kinds = kinds
            .GroupBy(k => k.Name)
            .Select(g => g.OrderByDescending(k => k.Severity).First())
            .OrderByDescending(k => k.Severity)
            .ToList();
        ch.Areas = areas.ToList();
        ch.Severity = top;
        if (ch.AreaName.Length == 0 && areas.Count > 0) ch.AreaName = areas.First();
    }

    /// <summary>Severity from a warning name's suffix. 特別警報 must be tested first.</summary>
    public static Severity SeverityOf(string name)
    {
        if (name.EndsWith("特別警報", StringComparison.Ordinal)) return Severity.Emergency;
        if (name.EndsWith("警報", StringComparison.Ordinal)) return Severity.Warning;
        if (name.EndsWith("注意報", StringComparison.Ordinal)) return Severity.Advisory;
        return Severity.None;
    }

    private static string Local(XElement parent, string localName) =>
        parent.Elements().FirstOrDefault(e => e.Name.LocalName == localName)?.Value?.Trim() ?? "";

    private static string GetString(JsonElement o, string name) =>
        o.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? (v.GetString() ?? "") : "";

    private static bool GetBool(JsonElement o, string name) =>
        o.TryGetProperty(name, out var v) && (v.ValueKind == JsonValueKind.True);

    private static long GetLong(JsonElement o, string name) =>
        o.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.Number ? v.GetInt64() : 0;
}
