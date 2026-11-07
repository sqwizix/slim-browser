using System.Text;
using Microsoft.Web.WebView2.Core;

namespace SlimBrowser;

internal static class SelfTest
{
    public static void Run()
    {
        var results = new List<string>();
        var directory = Path.Combine(Path.GetTempPath(), "SlimBrowser-test-" + Guid.NewGuid());
        var originalDirectory = AppSettings.TestDirectory;
        Directory.CreateDirectory(directory);
        void Check(bool condition, string name) { if (!condition) throw new Exception(name); results.Add("PASS " + name); }
        try
        {
            AppSettings.TestDirectory=directory;
            var settingsPath=Path.Combine(directory,"settings.json");
            File.WriteAllText(settingsPath,"{\"ClearOnExit\":true}");
            var migrated=AppSettings.Load();
            Check(migrated.ClearCookiesOnExit&&migrated.ClearCacheOnExit,"Legacy cleanup switch migrates to both options");
            File.WriteAllText(settingsPath,"{\"ClearOnExit\":true,\"ClearCookiesOnExit\":false,\"ClearCacheOnExit\":true,\"GlassTransparency\":999}");
            var independent=AppSettings.Load();
            Check(!independent.ClearCookiesOnExit&&independent.ClearCacheOnExit,"Explicit cleanup choices override legacy switch");
            Check(independent.GlassTransparency==100,"Transparency clamps invalid saved values");
            independent.SidebarVisible=false;independent.GlassTransparency=27;independent.Save();
            var restored=AppSettings.Load();
            Check(!restored.SidebarVisible&&restored.GlassTransparency==27,"Sidebar and transparency persist");
            Check(!File.ReadAllText(settingsPath).Contains("\"ClearOnExit\""),"Legacy setting retired on save");
            Check(BrowserEngine.CleanupKinds(true,false)==CoreWebView2BrowsingDataKinds.Cookies,"Cookies-only cleanup excludes cache");
            Check(BrowserEngine.CleanupKinds(false,true)==(CoreWebView2BrowsingDataKinds.DiskCache|CoreWebView2BrowsingDataKinds.CacheStorage),"Cache-only cleanup excludes cookies");
            Check(BrowserEngine.CleanupKinds(false,false)==0,"Disabled cleanup clears nothing");
            Check(Navigation.Resolve("") == Navigation.Home, "Empty input opens local start page");
            Check(Navigation.Resolve("github.com") == "https://github.com/", "Bare domain");
            Check(Navigation.Resolve("http://localhost:8080/test") == "http://localhost:8080/test", "HTTP localhost and port");
            Check(Navigation.Resolve("localhost:3000") == "https://localhost:3000/", "Bare localhost and port");
            Check(Navigation.Resolve("127.0.0.1:8080") == "https://127.0.0.1:8080/", "IPv4");
            Check(Navigation.Resolve("пример.рф").StartsWith("https://"), "Unicode domain");
            Check(Navigation.Resolve("кот & пес") == "https://www.google.com/search?q=" + Uri.EscapeDataString("кот & пес"), "Encoded query");
            Check(Navigation.Resolve("javascript:alert(1)").StartsWith("https://www.google.com/search?"), "No script navigation");
            Check(!Navigation.IsWeb("https://user:secret@example.com"), "Reject URL credentials");
            Check(Navigation.SecureOrigin("http://example.com") == null, "No password on HTTP");
            Check(Navigation.SecureOrigin("https://example.com:8443/login") == "https://example.com:8443", "Port isolation");
            var path = Path.Combine(directory, "vault"); var vault = new PasswordManager(path);
            var item = new Credential("https://example.com/login", "test-user", "unique-secret-123!");
            vault.Save(item);
            Check(!Encoding.UTF8.GetString(File.ReadAllBytes(path)).Contains(item.Password), "No plaintext password on disk");
            Check(vault.Read().Single().Password == item.Password, "DPAPI roundtrip");
            vault.Save(item with { Password = "changed" });
            Check(vault.Read().Count == 1 && vault.Read()[0].Password == "changed", "Update existing credential");
            vault.Delete(vault.Read()[0]); Check(vault.Read().Count == 0, "Delete credential");
            File.WriteAllBytes(path, [1, 2, 3]);
            try { vault.Read(); throw new Exception("Corruption accepted"); }
            catch (System.Security.Cryptography.CryptographicException) { results.Add("PASS Corrupted vault rejected"); }
            Environment.ExitCode = 0;
        }
        catch (Exception e) { results.Add("FAIL " + e); Environment.ExitCode = 1; }
        finally
        {
            AppSettings.TestDirectory=originalDirectory;
            foreach (var file in Directory.GetFiles(directory)) File.Delete(file);
            Directory.Delete(directory);
            File.WriteAllLines(Path.Combine(AppContext.BaseDirectory, "self-test.txt"), results);
        }
    }
}
