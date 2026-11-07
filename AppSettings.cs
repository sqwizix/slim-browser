using System.Text.Json;

namespace SlimBrowser;

internal sealed class AppSettings
{
    public bool Glass { get; set; } = true;
    int glassTransparency = 65;
    public int GlassTransparency { get => glassTransparency; set => glassTransparency = Math.Clamp(value, 0, 100); }
    public bool SidebarVisible { get; set; } = true;
    public string DownloadFolder { get; set; } = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
    public string Language { get; set; } = "ru-RU";
    public double Zoom { get; set; } = 1.0;
    public bool DarkMode { get; set; }
    public bool SavePasswords { get; set; } = true;
    public int DiscardMinutes { get; set; } = 5;
    public bool ClearCookiesOnExit { get; set; }
    public bool ClearCacheOnExit { get; set; }
    internal static string? TestDirectory { get; set; }
    public static string DataDirectory => TestDirectory ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "SlimBrowser");
    static string FilePath => Path.Combine(DataDirectory, "settings.json");
    public static AppSettings Load()
    {
        Directory.CreateDirectory(DataDirectory);
        if (!File.Exists(FilePath)) return new();
        try
        {
            var json = File.ReadAllText(FilePath);
            var settings = JsonSerializer.Deserialize<AppSettings>(json) ?? new();
            using var document = JsonDocument.Parse(json);
            // Preserve the previous combined switch, without overwriting newer choices.
            if (document.RootElement.ValueKind == JsonValueKind.Object && document.RootElement.TryGetProperty("ClearOnExit", out var legacy) && legacy.ValueKind == JsonValueKind.True)
            {
                if (!document.RootElement.TryGetProperty(nameof(ClearCookiesOnExit), out _)) settings.ClearCookiesOnExit = true;
                if (!document.RootElement.TryGetProperty(nameof(ClearCacheOnExit), out _)) settings.ClearCacheOnExit = true;
            }
            if (settings.DiscardMinutes is not (0 or 5 or 15 or 30)) settings.DiscardMinutes = 5;
            return settings;
        }
        catch (JsonException) { MessageBox.Show("Файл настроек поврежден. Используются стандартные настройки."); return new(); }
    }
    public void Save() => AtomicWrite(FilePath, JsonSerializer.SerializeToUtf8Bytes(this));
    public static void AtomicWrite(string path, byte[] bytes)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temp = path + ".tmp";
        File.WriteAllBytes(temp, bytes);
        File.Move(temp, path, true);
    }
}
