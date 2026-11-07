namespace SlimBrowser;

internal static class Navigation
{
    public const string Home = "https://slim-browser.invalid/index.html";
    public static bool IsHome(string url) => url == Home;
    public static bool IsWeb(string value) => Uri.TryCreate(value, UriKind.Absolute, out var uri)
        && (uri.Scheme == "https" || uri.Scheme == "http") && !string.IsNullOrEmpty(uri.Host) && string.IsNullOrEmpty(uri.UserInfo);
    public static string Resolve(string input)
    {
        var text = input.Trim();
        if (text.Length == 0) return Home;
        if (!text.Any(char.IsWhiteSpace))
        {
            if (IsWeb(text)) return new Uri(text).AbsoluteUri;
            if (!text.Contains("://") && IsWeb("https://" + text))
            {
                var uri = new Uri("https://" + text);
                if (uri.Host.Contains('.') || uri.Host == "localhost" || uri.HostNameType is UriHostNameType.IPv4 or UriHostNameType.IPv6)
                    return uri.AbsoluteUri;
            }
        }
        return "https://www.google.com/search?q=" + Uri.EscapeDataString(text);
    }
    public static string? SecureOrigin(string url) => IsWeb(url) && new Uri(url).Scheme == "https"
        ? new Uri(url).GetLeftPart(UriPartial.Authority) : null;
}
