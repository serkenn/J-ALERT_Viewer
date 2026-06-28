using System.Net.Sockets;
using System.Text;

namespace JAlertReceiver;

/// <summary>
/// Connects to the SDR# J-Alert plugin's TCP JSONL sink (default 127.0.0.1:7355)
/// and reads newline-delimited JSON records, reconnecting forever on failure.
/// Each complete line is handed to the supplied callback.
/// </summary>
public sealed class JsonlTcpClient
{
    private readonly string _host;
    private readonly int _port;
    private readonly Action<string> _onLine;
    private readonly Action<bool> _onConnected;

    public JsonlTcpClient(string host, int port, Action<string> onLine, Action<bool> onConnected)
    {
        _host = host;
        _port = port;
        _onLine = onLine;
        _onConnected = onConnected;
    }

    public async Task RunAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            try
            {
                using var client = new TcpClient();
                await client.ConnectAsync(_host, _port, ct);
                client.NoDelay = true;
                _onConnected(true);
                Console.WriteLine($"[tcp] connected to {_host}:{_port}");

                using var stream = client.GetStream();
                await ReadLinesAsync(stream, ct);
            }
            catch (OperationCanceledException) { break; }
            catch (Exception ex)
            {
                Console.WriteLine($"[tcp] {ex.Message}");
            }

            _onConnected(false);
            try { await Task.Delay(2000, ct); } catch { break; }
        }
    }

    private async Task ReadLinesAsync(NetworkStream stream, CancellationToken ct)
    {
        var buf = new byte[16 * 1024];
        var sb = new StringBuilder();
        var decoder = Encoding.UTF8.GetDecoder();
        var chars = new char[buf.Length];

        while (!ct.IsCancellationRequested)
        {
            int n = await stream.ReadAsync(buf, ct);
            if (n <= 0) break;  // peer closed

            int cc = decoder.GetChars(buf, 0, n, chars, 0);
            for (int i = 0; i < cc; i++)
            {
                char c = chars[i];
                if (c == '\n')
                {
                    string line = sb.ToString().TrimEnd('\r');
                    sb.Clear();
                    if (line.Length > 0) _onLine(line);
                }
                else
                {
                    sb.Append(c);
                }
            }
        }
    }
}
