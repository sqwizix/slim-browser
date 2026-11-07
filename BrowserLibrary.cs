using System.Text.Json;
namespace SlimBrowser;
internal sealed record SiteEntry(string Title,string Url,DateTime Visited);
internal sealed class BrowserLibrary
{
    string PathFor(string name)=>Path.Combine(AppSettings.DataDirectory,name+".json");
    public List<SiteEntry> Read(string name)
    {
        var path=PathFor(name);if(!File.Exists(path))return [];
        try{return JsonSerializer.Deserialize<List<SiteEntry>>(File.ReadAllText(path))??[];}catch(JsonException){return [];}
    }
    public void Add(string name,string title,string url)
    {
        if(!Navigation.IsWeb(url)||Navigation.IsHome(url))return;
        var items=Read(name);items.RemoveAll(x=>x.Url==url);items.Insert(0,new(title,url,DateTime.Now));
        AppSettings.AtomicWrite(PathFor(name),JsonSerializer.SerializeToUtf8Bytes(items.Take(name=="history"?1000:5000)));
    }
    public void Clear(string name)=>AppSettings.AtomicWrite(PathFor(name),JsonSerializer.SerializeToUtf8Bytes(Array.Empty<SiteEntry>()));
}
internal sealed class LibraryForm : MacWindow
{
    public LibraryForm(string title,IEnumerable<SiteEntry> entries,Action<string> navigate,Action clear)
    {
        Text=title;Size=new Size(800,600);MinimumSize=new Size(600,400);Font=new Font("Segoe UI",10);StartPosition=FormStartPosition.CenterParent;
        var list=new ListBox { Dock=DockStyle.Fill,BorderStyle=BorderStyle.None,DisplayMember="Title",IntegralHeight=false };
        foreach(var entry in entries)list.Items.Add(entry);
        list.DoubleClick+=(_,_)=>{if(list.SelectedItem is SiteEntry entry){navigate(entry.Url);Close();}};
        var button=new SoftButton {Text="Очистить список",Dock=DockStyle.Bottom,Height=44};
        button.Click+=(_,_)=>{if(MessageBox.Show(this,"Очистить этот список?",title,MessageBoxButtons.YesNo)==DialogResult.Yes){clear();list.Items.Clear();}};
        var hint=new MaterialLabel {Text="Двойной щелчок — открыть страницу",Dock=DockStyle.Top,Height=40,Padding=new Padding(14,10,0,0)};
        Controls.Add(list);Controls.Add(hint);Controls.Add(button);Controls.Add(MakeTitleBar(title));ChromeTheme.Apply(this);
    }
}
