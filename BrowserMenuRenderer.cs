using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;

namespace SlimBrowser;

internal sealed class BrowserMenu : ContextMenuStrip
{
    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(IntPtr handle,int attribute,ref int value,int size);
    [StructLayout(LayoutKind.Sequential)] struct Margins { public int Left,Right,Top,Bottom; }
    [DllImport("dwmapi.dll")] static extern int DwmExtendFrameIntoClientArea(IntPtr handle,ref Margins margins);
    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        int dark=ChromeTheme.Dark?1:0;DwmSetWindowAttribute(Handle,20,ref dark,4);
        int round=2;DwmSetWindowAttribute(Handle,33,ref round,4);
        int backdrop=ChromeTheme.UseGlass?3:1;DwmSetWindowAttribute(Handle,38,ref backdrop,4);
        var margins=new Margins{Left=-1,Right=-1,Top=-1,Bottom=-1};if(ChromeTheme.UseGlass)DwmExtendFrameIntoClientArea(Handle,ref margins);
    }
    protected override void OnSizeChanged(EventArgs e)
    {
        base.OnSizeChanged(e);if(Width<30||Height<30)return;
        using var outline=ChromeTheme.Round(new Rectangle(0,0,Width,Height),14);
        var previous=Region;Region=new Region(outline);previous?.Dispose();
    }
}

internal sealed class BrowserMenuRenderer : ToolStripProfessionalRenderer
{
    protected override void OnRenderImageMargin(ToolStripRenderEventArgs e) { }
    protected override void OnRenderToolStripBackground(ToolStripRenderEventArgs e)
    {
        e.Graphics.CompositingMode=CompositingMode.SourceCopy;
        e.Graphics.Clear(ChromeTheme.UseGlass?Color.FromArgb(Math.Max(140,(int)ChromeTheme.Material.A),ChromeTheme.SolidSurface):ChromeTheme.SolidSurface);
        e.Graphics.CompositingMode=CompositingMode.SourceOver;e.Graphics.SmoothingMode=SmoothingMode.AntiAlias;
        e.Graphics.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
    }
    protected override void OnRenderMenuItemBackground(ToolStripItemRenderEventArgs e)
    {
        if(!e.Item.Selected)return;
        e.Graphics.SmoothingMode=SmoothingMode.AntiAlias;
        using var shape=ChromeTheme.Round(new Rectangle(2,1,e.Item.Width-4,e.Item.Height-2),8);
        using var fill=new SolidBrush(Color.FromArgb(ChromeTheme.Dark?32:20,ChromeTheme.Dark?Color.White:Color.Black));e.Graphics.FillPath(fill,shape);
    }
    protected override void OnRenderToolStripBorder(ToolStripRenderEventArgs e)
    {
        e.Graphics.SmoothingMode=SmoothingMode.AntiAlias;
        using var outline=ChromeTheme.Round(new Rectangle(0,0,e.ToolStrip.Width-1,e.ToolStrip.Height-1),14);
        using var border=new Pen(Color.FromArgb(ChromeTheme.Dark?50:110,Color.White));e.Graphics.DrawPath(border,outline);
    }
    protected override void OnRenderSeparator(ToolStripSeparatorRenderEventArgs e)
    {
        using var line=new Pen(Color.FromArgb(40,ChromeTheme.Ink));e.Graphics.DrawLine(line,12,e.Item.Height/2,e.Item.Width-12,e.Item.Height/2);
    }
    protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
    {
        using var ink=new SolidBrush(e.Item.Enabled?ChromeTheme.Ink:SystemColors.GrayText);
        using var format=new StringFormat{LineAlignment=StringAlignment.Center,FormatFlags=StringFormatFlags.NoWrap};
        if((e.TextFormat&TextFormatFlags.Right)!=0)format.Alignment=StringAlignment.Far;
        e.Graphics.DrawString(e.Text,e.TextFont??e.Item.Font,ink,e.TextRectangle,format);
    }
}
