using Microsoft.Web.WebView2.Core;
namespace SlimBrowser;

internal sealed class SettingsForm : MacWindow
{
    readonly AppSettings settings; readonly PasswordManager vault; readonly Action changed,clean;
    readonly CoreWebView2? core; readonly string current;
    readonly FlowLayoutPanel content=new MaterialFlowPanel(){Dock=DockStyle.Fill,AutoScroll=true,FlowDirection=FlowDirection.TopDown,WrapContents=false,Padding=new Padding(30,20,20,24)};
    readonly FlowLayoutPanel nav=new MaterialFlowPanel(){Dock=DockStyle.Left,Width=240,FlowDirection=FlowDirection.TopDown,WrapContents=false,Padding=new Padding(16,20,12,16)};
    readonly Dictionary<string,Button> buttons=[];
    string selectedSection="Внешний вид";
    readonly string[] sections=["Внешний вид","Управление вкладками","Приватность и защита","Пароли и автозаполнение","Загрузки","Вкладки и просмотр","Языки","Разрешения и данные","Расширения и темы"];
    public SettingsForm(AppSettings settings,PasswordManager vault,string current,Action changed,Action clean,CoreWebView2? core=null,string section="Внешний вид")
    {
        this.settings=settings;this.vault=vault;this.current=current;this.changed=changed;this.clean=clean;this.core=core;
        Text="Настройки · SlimBrowser";Font=new Font("Segoe UI",10);Size=new Size(1020,760);MinimumSize=new Size(940,700);StartPosition=FormStartPosition.CenterParent;
        nav.Controls.Add(new MaterialLabel{Text="Настройки",Font=new Font("Segoe UI",20,FontStyle.Bold),AutoSize=true,Margin=new Padding(6,0,0,24)});
        foreach(var title in sections)
        {
            var button=new SoftButton{Text=title,AutoSize=false,NavigationItem=true,Width=206,Height=40,FlatStyle=FlatStyle.Flat,TextAlign=ContentAlignment.MiddleLeft,Padding=new Padding(10,0,0,0),Margin=new Padding(0,2,0,2)};
            button.FlatAppearance.BorderSize=0;button.Click+=(_,_)=>Select(title);buttons[title]=button;nav.Controls.Add(button);
        }
        Controls.Add(content);Controls.Add(nav);Controls.Add(MakeTitleBar(""));Select(sections.Contains(section)?section:sections[0]);
    }
    void Save(){settings.Save();changed();Style();}
    void Style()
    {
        ChromeTheme.Apply(this);ApplyMaterial(settings.Glass);
        foreach(var pair in buttons){((SoftButton)pair.Value).Selected=pair.Key==selectedSection;pair.Value.Invalidate();}
    }
    void Note(string text)=>content.Controls.Add(new MaterialLabel{Text=text,AutoSize=true,MaximumSize=new Size(620,0),Margin=new Padding(0,8,0,18)});
    Button ActionButton(string text,Action action)
    {
        var b=new SoftButton{Text=text,AutoSize=true,Margin=new Padding(0,7,8,7)};b.Click+=(_,_)=>{try{action();}catch(Exception e){MessageBox.Show(this,e.Message,"Настройки");}};content.Controls.Add(b);return b;
    }
    void AsyncButton(string text,Func<Task> action)
    {
        var b=new SoftButton{Text=text,AutoSize=true,Margin=new Padding(0,7,8,7)};
        b.Click+=async(_,_)=>{b.Enabled=false;try{await action();}catch(Exception e){MessageBox.Show(this,e.Message,"Настройки");}finally{if(!b.IsDisposed)b.Enabled=true;}};content.Controls.Add(b);
    }
    void Toggle(string text,bool value,Action<bool> action)
    {
        var box=new MaterialToggle{Text=text,AccessibleName=text,Checked=value,Margin=new Padding(0,6,0,6)};box.CheckedChanged+=(_,_)=>{action(box.Checked);Save();};content.Controls.Add(box);
    }
    void Choice(string[] values,int selected,Action<int> action)
    {
        var box=new ComboBox{DropDownStyle=ComboBoxStyle.DropDownList,FlatStyle=FlatStyle.Flat,Width=280,Margin=new Padding(0,8,0,14)};box.Items.AddRange(values);box.SelectedIndex=selected;box.SelectedIndexChanged+=(_,_)=>{action(box.SelectedIndex);Save();};content.Controls.Add(box);
    }
    void Select(string section)
    {
        selectedSection=section;
        content.SuspendLayout();foreach(Control c in content.Controls.Cast<Control>().ToArray()){content.Controls.Remove(c);c.Dispose();}
        content.Controls.Add(new MaterialLabel{Text=section,AutoSize=true,Font=new Font("Segoe UI",21,FontStyle.Bold),Margin=new Padding(0,0,0,16)});
        switch(section)
        {
            case "Внешний вид":
                Note("Выберите оформление браузера.");
                var themes=new MaterialFlowPanel{AutoSize=true,Margin=Padding.Empty};
                var light=new ThemeChoice{Text="Светлая",Checked=!settings.DarkMode};var dark=new ThemeChoice{Text="Темная",Checked=settings.DarkMode};
                themes.Controls.Add(light);themes.Controls.Add(dark);content.Controls.Add(themes);
                light.CheckedChanged+=(_,_)=>{if(light.Checked){settings.DarkMode=false;Save();}};dark.CheckedChanged+=(_,_)=>{if(dark.Checked){settings.DarkMode=true;Save();}};
                Toggle("Стеклянный интерфейс",settings.Glass,v=>settings.Glass=v);
                Note("Прозрачность");
                var amount=new MaterialLabel{Text=$"{settings.GlassTransparency} %",AutoSize=true,Margin=new Padding(0,0,0,0)};content.Controls.Add(amount);
                var slider=new MaterialSlider{Value=settings.GlassTransparency,Margin=new Padding(0,0,0,0)};
                slider.ValueChanged+=(_,_)=>{settings.GlassTransparency=slider.Value;settings.Glass=true;amount.Text=$"{slider.Value} %";foreach(var toggle in content.Controls.OfType<MaterialToggle>())toggle.Checked=true;changed();Style();};
                slider.ValueCommitted+=(_,_)=>settings.Save();content.Controls.Add(slider);
                var scale=new MaterialTablePanel{Width=590,Height=24,ColumnCount=2,RowCount=1,Margin=new Padding(0,0,0,16)};
                scale.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,50));scale.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,50));
                scale.Controls.Add(new MaterialLabel{Text="Матовый",Dock=DockStyle.Fill},0,0);scale.Controls.Add(new MaterialLabel{Text="Прозрачный",Dock=DockStyle.Fill,TextAlign=ContentAlignment.MiddleRight},1,0);content.Controls.Add(scale);
                Note("Прозрачность применяется к фону окна, вкладкам, кнопкам, меню и настройкам. Текст и страницы остаются чёткими. Размытие зависит от эффектов прозрачности Windows.");break;
            case "Управление вкладками":
                Note("Вкладки находятся слева. Плюс открывает новую вкладку, крестик закрывает выбранную.");
                Note("Выгружать неактивные вкладки через:");var times=new[]{5,15,30,0};Choice(["5 минут","15 минут","30 минут","Никогда"],Array.IndexOf(times,settings.DiscardMinutes),i=>settings.DiscardMinutes=times[i]);
                Note("После выгрузки страница загружается заново; несохраненные формы теряются. Загрузки файлов не прерываются.");ActionButton("Освободить память",clean);break;
            case "Приватность и защита":
                Toggle("Очищать cookies при выходе",settings.ClearCookiesOnExit,v=>settings.ClearCookiesOnExit=v);
                Toggle("Очищать кэш при выходе",settings.ClearCacheOnExit,v=>settings.ClearCacheOnExit=v);
                Note("Cookies отвечают за вход на сайты; кэш хранит загруженные файлы. Каждый пункт действует независимо при закрытии последнего обычного окна.");
                Note("Приватное окно использует InPrivate-профиль: журнал не записывается на диск, пароли не заполняются. Скачанные файлы и явно добавленные закладки сохраняются.");
                ActionButton("Открыть приватное окно",()=>BrowserApplication.Current?.Open(privateMode:true));break;
            case "Пароли и автозаполнение": Passwords();break;
            case "Загрузки":
                Note("Папка для новых загрузок:");Note(settings.DownloadFolder);
                ActionButton("Выбрать папку",()=>{using var picker=new FolderBrowserDialog{InitialDirectory=settings.DownloadFolder};if(picker.ShowDialog(this)==DialogResult.OK){settings.DownloadFolder=picker.SelectedPath;Save();Select(section);}});
                ActionButton("Открыть папку",()=>System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(settings.DownloadFolder){UseShellExecute=true}));
                ActionButton("Показать загрузки",()=>core?.OpenDefaultDownloadDialog());break;
            case "Вкладки и просмотр":
                Note("Масштаб страниц:");double[] zooms=[.75,.9,1,1.1,1.25,1.5];Choice(["75 %","90 %","100 %","110 %","125 %","150 %"],Math.Max(0,Array.IndexOf(zooms,settings.Zoom)),i=>settings.Zoom=zooms[i]);
                Note("Масштаб применяется к открытым и новым страницам.");break;
            case "Языки":
                Note("Язык веб-движка (после закрытия всех окон и повторного запуска):");
                Choice(["Русский","English","Беларуская"],Math.Max(0,Array.IndexOf(new[]{"ru-RU","en-US","be-BY"},settings.Language)),i=>settings.Language=new[]{"ru-RU","en-US","be-BY"}[i]);
                Note("Интерфейс SlimBrowser — на русском. Язык отдельных сайтов выбирается самими сайтами.");break;
            case "Разрешения и данные":
                Note("Камера, микрофон и другие разрешения запрашиваются для конкретного сайта. Фоновые запросы отклоняются.");
                AsyncButton("Сбросить разрешения сайтов",async()=>{if(core==null)return;foreach(var p in await core.Profile.GetNonDefaultPermissionSettingsAsync())await core.Profile.SetPermissionStateAsync(p.PermissionKind,p.PermissionOrigin,CoreWebView2PermissionState.Default);MessageBox.Show(this,"Разрешения сброшены.");});
                AsyncButton("Очистить cookies и кэш",async()=>{if(core!=null&&MessageBox.Show(this,"Сайты могут запросить повторный вход. Продолжить?","Очистка",MessageBoxButtons.YesNo)==DialogResult.Yes)await BrowserEngine.ClearAsync(core,true);});break;
            case "Расширения и темы": Extensions();break;
        }
        Style();content.ResumeLayout();
    }
    void Extensions()
    {
        Note("Темы находятся в разделе «Внешний вид». Расширения устанавливаются из локальной папки с manifest.json. Совместимость определяется WebView2.");
        var list=new ListBox{Width=610,Height=210,BorderStyle=BorderStyle.None,DisplayMember="Name"};content.Controls.Add(list);
        async Task Reload(){if(core==null)return;var items=await core.Profile.GetBrowserExtensionsAsync();if(list.IsDisposed)return;list.Items.Clear();foreach(var item in items)list.Items.Add(item);}
        AsyncButton("Обновить список",Reload);
        AsyncButton("Установить из папки",async()=>{if(core==null)return;using var picker=new FolderBrowserDialog();if(picker.ShowDialog(this)==DialogResult.OK&&MessageBox.Show(this,"Установить доверенное расширение? Оно получит доступ согласно manifest.json.","Расширение",MessageBoxButtons.YesNo)==DialogResult.Yes){await core.Profile.AddBrowserExtensionAsync(picker.SelectedPath);await Reload();}});
        AsyncButton("Включить / выключить",async()=>{if(list.SelectedItem is CoreWebView2BrowserExtension ex){await ex.EnableAsync(!ex.IsEnabled);await Reload();}});
        AsyncButton("Удалить расширение",async()=>{if(list.SelectedItem is CoreWebView2BrowserExtension ex&&MessageBox.Show(this,"Удалить "+ex.Name+"?","Расширение",MessageBoxButtons.YesNo)==DialogResult.Yes){await ex.RemoveAsync();await Reload();}});
        async void Load(){try{await Reload();}catch(Exception e){if(!IsDisposed)MessageBox.Show(this,e.Message);}} Load();
    }
    void Passwords()
    {
        Toggle("Сохранять и заполнять пароли",settings.SavePasswords,v=>settings.SavePasswords=v);
        var list=new ListBox{Width=610,Height=140,BorderStyle=BorderStyle.None,DisplayMember="Origin"};content.Controls.Add(list);
        void Reload(){list.Items.Clear();foreach(var item in vault.Read())list.Items.Add(item);}
        try{Reload();}catch(Exception e){Note("Хранилище не изменено: "+e.Message);}
        ActionButton("Копировать пароль",()=>{if(list.SelectedItem is Credential item)Clipboard.SetText(item.Password);});
        ActionButton("Удалить запись",()=>{if(list.SelectedItem is Credential item&&MessageBox.Show(this,"Удалить "+item.Origin+"?","Пароли",MessageBoxButtons.YesNo)==DialogResult.Yes){vault.Delete(item);Reload();}});
        TextBox Input(string placeholder,bool secret=false){var field=new TextBox{BorderStyle=BorderStyle.None,Dock=DockStyle.Fill,PlaceholderText=placeholder,UseSystemPasswordChar=secret,AccessibleName=placeholder};var shell=new RoundedPanel{Width=610,Height=38,Padding=new Padding(12,8,12,6),Margin=new Padding(0,4,0,4)};shell.Controls.Add(field);content.Controls.Add(shell);return field;}
        var site=Input("https://example.com");site.Text=Navigation.IsHome(current)?"":Navigation.SecureOrigin(current)??"";var user=Input("Логин");var password=Input("Пароль",true);
        ActionButton("Сохранить пароль",()=>{if(!settings.SavePasswords)throw new InvalidOperationException("Включите сохранение паролей.");if(string.IsNullOrWhiteSpace(user.Text)||password.Text.Length==0)throw new ArgumentException("Введите логин и пароль.");vault.Save(new(site.Text.Trim(),user.Text,password.Text));password.Clear();Reload();});
        Note("Пароли шифруются Windows DPAPI. Скопированный пароль остается в буфере обмена.");
    }
}
