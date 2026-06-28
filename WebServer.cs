using System.Collections.Concurrent;
using System.Net;
using System.Reflection;
using System.Text;
using System.Text.Json;

namespace JAlertReceiver;

/// <summary>
/// Minimal dependency-free web server (HttpListener) that drives the display:
///   GET /            -> the embedded single-page UI
///   GET /events      -> Server-Sent Events stream of <see cref="StateSnapshot"/>
///   GET /api/state   -> current snapshot as JSON (one-shot)
///   GET /api/xml?key -> full JMA XML for a channel
/// </summary>
public sealed class WebServer
{
    private static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
    };

    private readonly AlertState _state;
    private readonly int _port;
    private readonly HttpListener _listener = new();
    private readonly ConcurrentDictionary<StreamWriter, byte> _sse = new();
    private readonly string _indexHtml;
    private readonly string _inboxHtml;

    public WebServer(AlertState state, int port)
    {
        _state = state;
        _port = port;
        _indexHtml = LoadEmbedded("wwwroot.index.html");
        _inboxHtml = LoadEmbedded("wwwroot.inbox.html");
        _state.Changed += Broadcast;
    }

    public void Start()
    {
        // "+" binds all interfaces (works unprivileged on Linux for high ports;
        // on Windows may need: netsh http add urlacl url=http://+:PORT/ user=...).
        foreach (string prefix in new[] { $"http://+:{_port}/", $"http://localhost:{_port}/" })
        {
            try
            {
                _listener.Prefixes.Clear();
                _listener.Prefixes.Add(prefix);
                _listener.Start();
                Console.WriteLine($"[web] listening on {prefix}");
                _ = AcceptLoop();
                return;
            }
            catch (HttpListenerException ex)
            {
                Console.WriteLine($"[web] cannot bind {prefix}: {ex.Message}");
            }
        }
        throw new InvalidOperationException($"Could not start web server on port {_port}.");
    }

    private async Task AcceptLoop()
    {
        while (_listener.IsListening)
        {
            HttpListenerContext ctx;
            try { ctx = await _listener.GetContextAsync(); }
            catch { return; }
            _ = Task.Run(() => Handle(ctx));
        }
    }

    private void Handle(HttpListenerContext ctx)
    {
        string path = ctx.Request.Url?.AbsolutePath ?? "/";
        try
        {
            switch (path)
            {
                case "/": SendHtml(ctx, _indexHtml); break;
                case "/inbox": SendHtml(ctx, _inboxHtml); break;
                case "/api/state": SendJson(ctx, _state.Snapshot()); break;
                case "/api/xml": SendXml(ctx); break;
                case "/api/read": HandleRead(ctx); break;
                case "/events": ServeSse(ctx); break;
                case "/healthz": SendText(ctx, "ok"); break;
                default: ctx.Response.StatusCode = 404; ctx.Response.Close(); break;
            }
        }
        catch
        {
            try { ctx.Response.Abort(); } catch { }
        }
    }

    private void ServeSse(HttpListenerContext ctx)
    {
        var res = ctx.Response;
        res.StatusCode = 200;
        res.SendChunked = true;
        res.ContentType = "text/event-stream; charset=utf-8";
        res.Headers["Cache-Control"] = "no-cache";
        res.Headers["X-Accel-Buffering"] = "no";

        var writer = new StreamWriter(res.OutputStream, new UTF8Encoding(false)) { AutoFlush = false };
        _sse[writer] = 0;
        try
        {
            WriteEvent(writer, _state.Snapshot());   // prime with current state
            // Keep the request thread alive; broadcasts happen from Broadcast().
            // A periodic comment doubles as a keep-alive / dead-peer detector.
            while (_sse.ContainsKey(writer))
            {
                Thread.Sleep(15000);
                lock (writer) { writer.Write(": ping\n\n"); writer.Flush(); }
            }
        }
        catch { }
        finally
        {
            _sse.TryRemove(writer, out _);
            try { res.Close(); } catch { }
        }
    }

    private void Broadcast(StateSnapshot snap)
    {
        foreach (var writer in _sse.Keys)
        {
            try { WriteEvent(writer, snap); }
            catch { _sse.TryRemove(writer, out _); }
        }
    }

    private static void WriteEvent(StreamWriter writer, StateSnapshot snap)
    {
        string data = JsonSerializer.Serialize(snap, Json);
        lock (writer)
        {
            writer.Write("data: ");
            writer.Write(data);
            writer.Write("\n\n");
            writer.Flush();
        }
    }

    private void SendXml(HttpListenerContext ctx)
    {
        string? xml = long.TryParse(ctx.Request.QueryString["id"], out long id)
            ? _state.XmlForId(id)
            : _state.XmlForKey(ctx.Request.QueryString["key"] ?? "");
        if (xml == null) { ctx.Response.StatusCode = 404; ctx.Response.Close(); return; }
        WriteBody(ctx, "application/xml; charset=utf-8", Encoding.UTF8.GetBytes(xml));
    }

    // POST /api/read?id=N&read=true|false   or   /api/read?all=true
    private void HandleRead(HttpListenerContext ctx)
    {
        var q = ctx.Request.QueryString;
        if (q["all"] == "true") _state.MarkAllRead();
        else if (long.TryParse(q["id"], out long id))
            _state.MarkRead(id, q["read"] != "false");
        SendText(ctx, "ok");
    }

    private static void SendHtml(HttpListenerContext ctx, string html) =>
        WriteBody(ctx, "text/html; charset=utf-8", Encoding.UTF8.GetBytes(html));

    private static void SendText(HttpListenerContext ctx, string text) =>
        WriteBody(ctx, "text/plain; charset=utf-8", Encoding.UTF8.GetBytes(text));

    private void SendJson(HttpListenerContext ctx, object obj) =>
        WriteBody(ctx, "application/json; charset=utf-8",
                  JsonSerializer.SerializeToUtf8Bytes(obj, Json));

    private static void WriteBody(HttpListenerContext ctx, string contentType, byte[] body)
    {
        ctx.Response.StatusCode = 200;
        ctx.Response.ContentType = contentType;
        ctx.Response.ContentLength64 = body.Length;
        ctx.Response.OutputStream.Write(body, 0, body.Length);
        ctx.Response.Close();
    }

    private static string LoadEmbedded(string nameTail)
    {
        Assembly asm = Assembly.GetExecutingAssembly();
        string? full = asm.GetManifestResourceNames().FirstOrDefault(n => n.EndsWith(nameTail, StringComparison.Ordinal));
        if (full == null) throw new FileNotFoundException($"Embedded resource *{nameTail} not found.");
        using var s = asm.GetManifestResourceStream(full)!;
        using var r = new StreamReader(s, Encoding.UTF8);
        return r.ReadToEnd();
    }
}
