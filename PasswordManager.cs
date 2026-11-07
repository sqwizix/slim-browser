using System.Security.Cryptography;
using System.Text.Json;

namespace SlimBrowser;

internal sealed record Credential(string Origin, string Username, string Password);
internal sealed class PasswordManager
{
    readonly string path;
    public PasswordManager(string? file = null) => path = file ?? Path.Combine(AppSettings.DataDirectory, "passwords.dpapi");
    public List<Credential> Read()
    {
        if (!File.Exists(path)) return [];
        var plain = ProtectedData.Unprotect(File.ReadAllBytes(path), null, DataProtectionScope.CurrentUser);
        try { return JsonSerializer.Deserialize<List<Credential>>(plain) ?? []; }
        finally { CryptographicOperations.ZeroMemory(plain); }
    }
    void Write(List<Credential> entries)
    {
        var plain = JsonSerializer.SerializeToUtf8Bytes(entries);
        try { AppSettings.AtomicWrite(path, ProtectedData.Protect(plain, null, DataProtectionScope.CurrentUser)); }
        finally { CryptographicOperations.ZeroMemory(plain); }
    }
    public void Save(Credential item)
    {
        var origin = Navigation.SecureOrigin(item.Origin) ?? throw new ArgumentException("Для паролей требуется адрес HTTPS.");
        var items = Read();
        items.RemoveAll(c => c.Origin == origin && c.Username == item.Username);
        items.Add(item with { Origin = origin });
        Write(items);
    }
    public void Delete(Credential item) { var items = Read(); items.RemoveAll(c => c.Origin == item.Origin && c.Username == item.Username); Write(items); }
    public async Task FillAsync(Microsoft.Web.WebView2.Core.CoreWebView2 core)
    {
        var origin = Navigation.SecureOrigin(core.Source);
        var items = Read().Where(c => c.Origin == origin).ToList();
        if (items.Count != 1) return;
        var item = items[0];
        var script = """
        (() => {
          const credential = CREDENTIAL;
          if (location.origin !== credential.Origin || window.top !== window) return;
          const visible = e => !e.disabled && !e.readOnly && e.getClientRects().length > 0;
          const password = [...document.querySelectorAll('input[type="password"]')].find(e => visible(e) && e.autocomplete !== 'new-password');
          if (!password || password.value) return;
          const scope = password.form || document;
          const username = [...scope.querySelectorAll('input')].find(e => visible(e) && (e.autocomplete === 'username' || e.type === 'email'))
            || [...scope.querySelectorAll('input[type="text"]')].find(visible);
          const set = (e, value) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(e, value); e.dispatchEvent(new Event('input', {bubbles:true})); e.dispatchEvent(new Event('change', {bubbles:true})); };
          if (username && !username.value) set(username, credential.Username);
          set(password, credential.Password);
        })();
        """;
        await core.ExecuteScriptAsync(script.Replace("CREDENTIAL", JsonSerializer.Serialize(item)));
    }
}
