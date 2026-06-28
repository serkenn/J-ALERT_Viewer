using JAlertReceiver;

// ---- Configuration (CLI flags / environment) -------------------------------
// --source-host H      plugin TCP host           (default 127.0.0.1, env JALERT_SOURCE_HOST)
// --source-port P      plugin TCP JSONL port     (default 7355,      env JALERT_SOURCE_PORT)
// --web-port P         web UI port               (default 8080,      env JALERT_WEB_PORT)
// --replay FILE        replay a .jsonl file instead of connecting (test mode)
// --replay-interval MS delay between replayed lines (default 800)
var opt = Options.Parse(args);

Console.WriteLine($"J-Alert Receiver  source={opt.SourceHost}:{opt.SourcePort}  web=:{opt.WebPort}"
                  + (opt.ReplayFile != null ? $"  replay={opt.ReplayFile}" : ""));

string source = opt.ReplayFile != null ? $"replay:{Path.GetFileName(opt.ReplayFile)}"
                                       : $"{opt.SourceHost}:{opt.SourcePort}";
var state = new AlertState(source);

void OnLine(string line)
{
    var ch = AlertClassifier.FromJsonLine(line);
    if (ch != null) state.Ingest(ch);
}

var web = new WebServer(state, opt.WebPort);
web.Start();
Console.WriteLine($"  表示画面  http://localhost:{opt.WebPort}/");
Console.WriteLine($"  受信箱    http://localhost:{opt.WebPort}/inbox");

System.Diagnostics.Process? tunnel = null;
if (opt.Cloudflared) tunnel = Cloudflared.Start(opt.CloudflaredBin, opt.WebPort);

using var cts = new CancellationTokenSource();
Console.CancelKeyPress += (_, e) =>
{
    e.Cancel = true;
    cts.Cancel();
    try { if (tunnel is { HasExited: false }) tunnel.Kill(entireProcessTree: true); } catch { }
};

if (opt.ReplayFile != null)
{
    state.SetConnected(true);
    await ReplayAsync(opt.ReplayFile, opt.ReplayInterval, OnLine, cts.Token);
    Console.WriteLine("[replay] done — UI stays live; Ctrl+C to quit.");
    try { await Task.Delay(Timeout.Infinite, cts.Token); } catch { }
}
else
{
    var client = new JsonlTcpClient(opt.SourceHost, opt.SourcePort, OnLine, state.SetConnected);
    await client.RunAsync(cts.Token);
}

static async Task ReplayAsync(string file, int intervalMs, Action<string> onLine, CancellationToken ct)
{
    foreach (string line in File.ReadLines(file))
    {
        if (ct.IsCancellationRequested) return;
        if (line.Trim().Length == 0) continue;
        onLine(line);
        try { await Task.Delay(intervalMs, ct); } catch { return; }
    }
}

sealed class Options
{
    public string SourceHost = Env("JALERT_SOURCE_HOST", "127.0.0.1");
    public int SourcePort = EnvInt("JALERT_SOURCE_PORT", 7355);
    public int WebPort = EnvInt("JALERT_WEB_PORT", 8080);
    public string? ReplayFile;
    public int ReplayInterval = 800;
    public bool Cloudflared = Env("JALERT_CLOUDFLARED", "") is "1" or "true";
    public string CloudflaredBin = Env("JALERT_CLOUDFLARED_BIN", "cloudflared");

    public static Options Parse(string[] a)
    {
        var o = new Options();
        for (int i = 0; i < a.Length; i++)
        {
            string k = a[i];
            string Next() => ++i < a.Length ? a[i] : throw new ArgumentException($"missing value for {k}");
            switch (k)
            {
                case "--source-host": o.SourceHost = Next(); break;
                case "--source-port": o.SourcePort = int.Parse(Next()); break;
                case "--web-port": o.WebPort = int.Parse(Next()); break;
                case "--replay": o.ReplayFile = Next(); break;
                case "--replay-interval": o.ReplayInterval = int.Parse(Next()); break;
                case "--cloudflared": o.Cloudflared = true; break;
                case "--cloudflared-bin": o.CloudflaredBin = Next(); break;
                case "-h": case "--help":
                    Console.WriteLine("usage: jalert-receiver [--source-host H] [--source-port P] [--web-port P]\n"
                        + "                       [--replay FILE] [--replay-interval MS]\n"
                        + "                       [--cloudflared] [--cloudflared-bin PATH]");
                    Environment.Exit(0); break;
                default: throw new ArgumentException($"unknown argument: {k}");
            }
        }
        return o;
    }

    static string Env(string n, string d) => Environment.GetEnvironmentVariable(n) is { Length: > 0 } v ? v : d;
    static int EnvInt(string n, int d) => int.TryParse(Environment.GetEnvironmentVariable(n), out var v) ? v : d;
}
