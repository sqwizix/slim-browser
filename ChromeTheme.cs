using System.Drawing.Drawing2D;

namespace SlimBrowser;

internal static class ChromeTheme
{
    public static bool Dark { get; set; }
    public static bool Glass { get; set; }
    public static int Transparency { get; set; } = 65;
    public static bool UseGlass => Glass && !SystemInformation.HighContrast;
    public static Color Accent => SystemInformation.HighContrast ? SystemColors.Highlight : Color.FromArgb(55, 132, 236);
    public static Color Material => UseGlass ? Color.FromArgb(245 - (int)(Math.Clamp(Transparency, 0, 100) * 2.2), SolidSurface) : SolidSurface;
    public static Color GlassField => UseGlass ? Color.FromArgb(Math.Max(100, (int)Material.A), Field) : Field;
    public static Color SolidSurface => SystemInformation.HighContrast ? SystemColors.Window : Dark ? Color.FromArgb(30,30,34) : Color.FromArgb(242,242,246);
    public static Color Surface => UseGlass ? Color.Black : SolidSurface;
    public static Color Field => SystemInformation.HighContrast ? SystemColors.Window : Dark ? Color.FromArgb(37,38,43) : Color.White;
    public static Color Border => SystemInformation.HighContrast ? SystemColors.WindowText : Dark ? Color.FromArgb(55,57,64) : Color.FromArgb(222,222,228);
    public static Color Ink => SystemInformation.HighContrast ? SystemColors.WindowText : Dark ? Color.FromArgb(241,242,245) : Color.FromArgb(36,38,43);
    public static void Apply(Control root)
    {
        root.BackColor = root is TextBox or ListView or ComboBox or RoundedPanel ? Field : Surface; root.ForeColor = Ink;
        foreach(Control child in root.Controls) Apply(child);
        root.Invalidate();
    }
    public static void ClearSurface(Graphics graphics, Control? parent)
    {
        var color = parent is RoundedPanel && parent.BackColor!=Color.Black ? GlassField : Material;
        graphics.CompositingMode = CompositingMode.SourceCopy; graphics.Clear(color); graphics.CompositingMode = CompositingMode.SourceOver;
        graphics.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
    }
    public static GraphicsPath Round(Rectangle r, int radius)
    {
        var path = new GraphicsPath(); var d = radius * 2;
        path.AddArc(r.X, r.Y, d, d, 180, 90); path.AddArc(r.Right-d, r.Y, d, d, 270, 90);
        path.AddArc(r.Right-d, r.Bottom-d, d, d, 0, 90); path.AddArc(r.X, r.Bottom-d, d, d, 90, 90); path.CloseFigure(); return path;
    }
}
internal sealed class RoundedPanel : Panel
{
    public RoundedPanel() { DoubleBuffered = true; ResizeRedraw = true; }
    protected override void OnPaintBackground(PaintEventArgs e)
    {
        MaterialPaint.Background(e.Graphics,this);
        if(BackColor==Color.Black)return;
        using var path=ChromeTheme.Round(new Rectangle(0,0,Width-1,Height-1),10);
        e.Graphics.CompositingMode=CompositingMode.SourceCopy;
        using var fill=new SolidBrush(ChromeTheme.GlassField);e.Graphics.FillPath(fill,path);
        e.Graphics.CompositingMode=CompositingMode.SourceOver;
        using var edge=new Pen(Color.FromArgb(ChromeTheme.Dark?46:130,ChromeTheme.Dark?Color.White:Color.White));e.Graphics.DrawPath(edge,path);
    }
    protected override void OnPaint(PaintEventArgs e) { base.OnPaint(e); }

}
internal sealed class ChromeButton : Button
{
    readonly string action;
    bool hover, down;
    [System.ComponentModel.DefaultValue(false)] public bool Selected { get; set; }
    [System.ComponentModel.DefaultValue(false)] public bool Loading { get; set; }
    public ChromeButton(string action)
    {
        this.action = action; AccessibleName = action; AccessibleRole = AccessibleRole.PushButton;
        Size = new Size(38, 40); Dock = DockStyle.Fill; Margin = new Padding(2,0,2,0);
        FlatStyle = FlatStyle.Flat; FlatAppearance.BorderSize = 0; BackColor = ChromeTheme.Surface;
        Cursor = Cursors.Hand; DoubleBuffered = true;
    }
    protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { hover = down = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e) { down = true; Invalidate(); base.OnMouseDown(e); }
    protected override void OnMouseUp(MouseEventArgs e) { down = false; Invalidate(); base.OnMouseUp(e); }
    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics; ChromeTheme.ClearSurface(g,Parent); g.SmoothingMode = SmoothingMode.AntiAlias;
        if (hover || (Focused && ShowFocusCues) || Selected) { using var shape = ChromeTheme.Round(new Rectangle(2,2,Width-5,Height-5),10); using var fill = new SolidBrush(Color.FromArgb(down?65:hover?35:22,ChromeTheme.Dark?Color.White:Color.Black)); g.FillPath(fill,shape); }
        g.TranslateTransform(Width/2f-10,Height/2f-10);
        using var p = new Pen(Enabled ? ChromeTheme.Ink : Color.FromArgb(95,ChromeTheme.Ink), 1.65f) { StartCap = LineCap.Round, EndCap = LineCap.Round, LineJoin = LineJoin.Round };
        switch(action)
        {
            case "Меню": using(var dot=new SolidBrush(ChromeTheme.Ink)){for(int x=3;x<=17;x+=7)g.FillEllipse(dot,x-1,9,2,2);} break;
            case "Боковая панель":
                using(var outline=ChromeTheme.Round(new Rectangle(2,3,16,14),3))g.DrawPath(p,outline);
                g.DrawLine(p,8,4,8,16);if(Selected){using var tint=new SolidBrush(Color.FromArgb(65,ChromeTheme.Ink));g.FillRectangle(tint,4,5,2,10);}break;
            case "Поиск": g.DrawEllipse(p,2,2,12,12);g.DrawLine(p,12,12,18,18);break;
            case "Назад": g.DrawLines(p,[new(12,3),new(5,10),new(12,17)]); break;
            case "Перейти":
            case "Вперед": g.DrawLines(p,[new(8,3),new(15,10),new(8,17)]); break;
            case "Обновить":
                if(Loading){using var stop=new SolidBrush(ChromeTheme.Ink);g.FillRectangle(stop,5,5,10,10);}
                else {g.DrawArc(p,3,3,14,14,-48,294);g.DrawLines(p,[new(12,3),new(17,3),new(17,8)]);}break;
            case "Новая": g.DrawLine(p,10,3,10,17); g.DrawLine(p,3,10,17,10); break;
            case "Закрыть": g.DrawLine(p,5,5,15,15); g.DrawLine(p,15,5,5,15); break;
            case "Заполнить": g.DrawEllipse(p,2,3,8,8); g.DrawLine(p,9,10,17,18); g.DrawLine(p,13,14,16,11); break;
            case "Очистить RAM": g.DrawRectangle(p,5,5,10,10); for(int i=7;i<=13;i+=6) {g.DrawLine(p,i,2,i,5);g.DrawLine(p,i,15,i,18);g.DrawLine(p,2,i,5,i);g.DrawLine(p,15,i,18,i);} break;
            default: g.DrawLine(p,3,5,17,5);g.DrawLine(p,3,15,17,15);g.FillEllipse(Brushes.White,6,2,6,6);g.DrawEllipse(p,6,2,6,6);g.FillEllipse(Brushes.White,10,12,6,6);g.DrawEllipse(p,10,12,6,6);break;
        }
        g.ResetTransform(); if(Focused && ShowFocusCues) {using var focus=ChromeTheme.Round(new Rectangle(2,2,Width-5,Height-5),10);using var pen=new Pen(ChromeTheme.Accent);g.DrawPath(pen,focus);}
    }
}
