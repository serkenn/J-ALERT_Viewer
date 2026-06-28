using System.Diagnostics;
using System.Text.RegularExpressions;

namespace JAlertReceiver;

/// <summary>
/// Optional Cloudflare Tunnel integration. Spawns <c>cloudflared</c> as a quick
/// tunnel in front of the local web port so the display / inbox can be reached
/// over the internet without opening firewall ports, and surfaces the public
/// *.trycloudflare.com URL. cloudflared must be installed and on PATH (or pass
/// an explicit path). No-op friendly: if it can't start, the local server keeps
/// running.
/// </summary>
public static class Cloudflared
{
    private static readonly Regex UrlRe =
        new(@"https://[a-z0-9-]+\.trycloudflare\.com", RegexOptions.IgnoreCase | RegexOptions.Compiled);

    public static Process? Start(string bin, int localPort)
    {
        var psi = new ProcessStartInfo
        {
            FileName = bin,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        psi.ArgumentList.Add("tunnel");
        psi.ArgumentList.Add("--no-autoupdate");
        psi.ArgumentList.Add("--url");
        psi.ArgumentList.Add($"http://localhost:{localPort}");

        Process proc;
        try { proc = Process.Start(psi)!; }
        catch (Exception ex)
        {
            Console.WriteLine($"[cloudflared] could not start '{bin}': {ex.Message}");
            Console.WriteLine("[cloudflared] install it from https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/");
            return null;
        }

        void Scan(string? line)
        {
            if (line == null) return;
            var m = UrlRe.Match(line);
            if (m.Success)
                Console.WriteLine($"\n========================================\n"
                                + $"  公開URL (Cloudflare Tunnel):\n  {m.Value}\n"
                                + $"========================================\n");
        }
        proc.OutputDataReceived += (_, e) => Scan(e.Data);
        proc.ErrorDataReceived += (_, e) => Scan(e.Data);
        proc.BeginOutputReadLine();
        proc.BeginErrorReadLine();

        Console.WriteLine($"[cloudflared] starting quick tunnel to http://localhost:{localPort} …");
        return proc;
    }
}
