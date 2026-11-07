namespace SlimBrowser;
internal sealed class StartPanel : MaterialPanel
{
    readonly RoundedPanel shell=new(){Size=new Size(600,56),Padding=new Padding(18,13,12,10)};
    readonly TextBox input=new GlassTextBox(){BorderStyle=BorderStyle.None,Dock=DockStyle.Fill,Font=new Font("Segoe UI",12),PlaceholderText="Поиск или адрес сайта",AccessibleName="Поиск или адрес сайта"};
    public StartPanel(Action<string> navigate)
    {
        Dock=DockStyle.Fill;BackColor=ChromeTheme.Surface;
        var go=new ChromeButton("Перейти"){Dock=DockStyle.Right,Width=34};
        input.KeyDown+=(_,e)=>{if(e.KeyCode==Keys.Enter){e.SuppressKeyPress=true;if(!string.IsNullOrWhiteSpace(input.Text))navigate(input.Text);}};
        go.Click+=(_,_)=>{if(!string.IsNullOrWhiteSpace(input.Text))navigate(input.Text);};
        shell.Controls.Add(input);shell.Controls.Add(go);Controls.Add(shell);Resize+=(_,_)=>Center();VisibleChanged+=(_,_)=>{if(Visible){Center();input.Focus();}};Center();
    }
    void Center(){shell.Width=Math.Min(600,Math.Max(200,ClientSize.Width-80));shell.Location=new Point((ClientSize.Width-shell.Width)/2,(ClientSize.Height-shell.Height)/2);}
}
internal sealed class TabSelectButton : Button
{
    public TabSelectButton(){FlatStyle=FlatStyle.Flat;FlatAppearance.BorderSize=0;}
    protected override void OnPaint(PaintEventArgs e)
    {
        ChromeTheme.ClearSurface(e.Graphics,Parent);
        using var brush=new SolidBrush(ChromeTheme.Ink);
        using var format=new StringFormat{LineAlignment=StringAlignment.Center,Trimming=StringTrimming.EllipsisCharacter,FormatFlags=StringFormatFlags.NoWrap};
        e.Graphics.DrawString(Text,Font,brush,new RectangleF(10,0,Width-14,Height),format);
        if(Focused && ShowFocusCues)ControlPaint.DrawFocusRectangle(e.Graphics,new Rectangle(3,3,Width-6,Height-6));
    }
}
