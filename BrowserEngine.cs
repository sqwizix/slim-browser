using Microsoft.Web.WebView2.Core;

namespace SlimBrowser;

internal sealed class BrowserEngine
{
    public Task<CoreWebView2Environment> Environment { get; }
    public BrowserEngine()
    {
        var options = new CoreWebView2EnvironmentOptions
        {
            AdditionalBrowserArguments = "--disk-cache-size=33554432 --media-cache-size=8388608 --disable-background-networking --disable-background-sync --disable-features=Prerender2,SpeculationRulesPrefetch,msEdgeSidebarV2 --no-first-run",
            AreBrowserExtensionsEnabled = true,
            Language = AppSettings.Load().Language,
            AllowSingleSignOnUsingOSPrimaryAccount = false
        };
        Environment = CoreWebView2Environment.CreateAsync(null, Path.Combine(AppSettings.DataDirectory, "WebView2"), options);
    }
    public static void Configure(CoreWebView2 core)
    {
        core.Settings.IsPasswordAutosaveEnabled = false;
        core.Settings.IsGeneralAutofillEnabled = false;
        core.Settings.IsWebMessageEnabled = false;
        core.Settings.AreHostObjectsAllowed = false;
        core.Profile.PreferredColorScheme = CoreWebView2PreferredColorScheme.Light;
    }
    internal static CoreWebView2BrowsingDataKinds CleanupKinds(bool cookies, bool cache) =>
        (cache ? CoreWebView2BrowsingDataKinds.DiskCache | CoreWebView2BrowsingDataKinds.CacheStorage : 0) |
        (cookies ? CoreWebView2BrowsingDataKinds.Cookies : 0);
    public static Task ClearAsync(CoreWebView2 core, bool cookies, bool cache = true)
    {
        var kinds = CleanupKinds(cookies, cache);
        return kinds == 0 ? Task.CompletedTask : core.Profile.ClearBrowsingDataAsync(kinds);
    }
}
