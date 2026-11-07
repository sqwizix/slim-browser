using System.Runtime.InteropServices;
using System.Drawing.Drawing2D;
namespace SlimBrowser;

internal class MacWindow : Form
{
    [DllImport("user32.dll")] static extern bool ReleaseCapture();
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h, int msg, IntPtr w, IntPtr l);
    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(IntPtr h,int attr,ref int value,int size);
    public MacWindow() { FormBorderStyle = FormBorderStyle.None; Padding = Padding.Empty; DoubleBuffered = true; }
    [StructLayout(LayoutKind.Sequential)] struct Margins { public int Left, Right, Top, Bottom; }
    [DllImport("dwmapi.dll")] static extern int DwmExtendFrameIntoClientArea(IntPtr h,ref Margins margins);
    public void ApplyMaterial(bool glass)
    {
        if(!IsHandleCreated)return;
        int dark=ChromeTheme.Dark?1:0; DwmSetWindowAttribute(Handle,20,ref dark,4);
        int round=2; DwmSetWindowAttribute(Handle,33,ref round,4);
        int border=-2; DwmSetWindowAttribute(Handle,34,ref border,4);
        glass &= !SystemInformation.HighContrast;
        int backdrop=glass?3:1; DwmSetWindowAttribute(Handle,38,ref backdrop,4);
        var margins=new Margins {Left=glass?-1:0,Right=glass?-1:0,Top=glass?-1:0,Bottom=glass?-1:0};
        DwmExtendFrameIntoClientArea(Handle,ref margins); Invalidate(true);
    }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics,this);
    protected override CreateParams CreateParams { get { var cp=base.CreateParams; cp.Style &= ~0xC00000; cp.Style |= 0x40000 | 0x20000 | 0x10000 | 0x80000; return cp; } }
    protected override void OnHandleCreated(EventArgs e) { base.OnHandleCreated(e); ApplyMaterial(ChromeTheme.UseGlass); MaximizedBounds=Screen.FromHandle(Handle).WorkingArea; }
    protected override void WndProc(ref Message m)
    {
        if(m.Msg==0x86) { m.LParam=(IntPtr)(-1); base.WndProc(ref m); return; }
        if(m.Msg==0x85) { m.Result=IntPtr.Zero; return; }
        if(m.Msg==0x83) { m.Result=IntPtr.Zero; return; }
        if(m.Msg==0x84 && WindowState==FormWindowState.Normal)
        {
            long pos=m.LParam.ToInt64(); var point=PointToClient(new Point((short)(pos&65535),(short)((pos>>16)&65535)));
            int b=Math.Max(6,DeviceDpi*6/96); bool l=point.X<b,r=point.X>=Width-b,t=point.Y<b,d=point.Y>=Height-b;
            int hit=t?(l?13:r?14:12):d?(l?16:r?17:15):l?10:r?11:0;
            if(hit!=0) { m.Result=(IntPtr)hit; return; }
        }
        base.WndProc(ref m);
    }
    public void ToggleMaximize() { MaximizedBounds=Screen.FromHandle(Handle).WorkingArea; WindowState=WindowState==FormWindowState.Maximized?FormWindowState.Normal:FormWindowState.Maximized; }
    protected Panel MakeTitleBar(string caption)
    {
        var title=new MaterialPanel { Dock=DockStyle.Top,Height=42,Padding=new Padding(14,7,14,5) };
        var lights=new MaterialFlowPanel { Dock=DockStyle.Left,Width=90,WrapContents=false };
        lights.Controls.Add(new WindowDot("Закрыть окно",Color.FromArgb(255,95,87),()=>Close()));
        lights.Controls.Add(new WindowDot("Развернуть или восстановить",Color.FromArgb(255,189,46),ToggleMaximize));
        lights.Controls.Add(new WindowDot("Свернуть окно",Color.FromArgb(40,200,64),()=>WindowState=FormWindowState.Minimized));
        var label=new MaterialLabel { Text=caption,Dock=DockStyle.Fill,TextAlign=ContentAlignment.MiddleCenter,Padding=new Padding(0,0,90,0) };
        title.Controls.Add(label);title.Controls.Add(lights);
        foreach(Control target in new Control[]{title,label})
        {
            target.MouseDown+=(_,e)=>{if(e.Button==MouseButtons.Left){ReleaseCapture();SendMessage(Handle,0xA1,(IntPtr)2,IntPtr.Zero);}};
            target.DoubleClick+=(_,_)=>ToggleMaximize();
        }
        return title;
    }
}
internal sealed class WindowDot : Button
{
    readonly Color color;
    public WindowDot(string name,Color color,Action action)
    {
        this.color=color; AccessibleName=name; Size=new Size(22,26);Margin=new Padding(0,0,6,0);FlatStyle=FlatStyle.Flat;FlatAppearance.BorderSize=0;Cursor=Cursors.Hand;Click+=(_,_)=>action();
    }
    protected override void OnPaint(PaintEventArgs e)
    {
        MaterialPaint.Background(e.Graphics,this);
        using var brush=new SolidBrush(color); e.Graphics.FillEllipse(brush,4,6,13,13);
        if(Focused && ShowFocusCues)ControlPaint.DrawFocusRectangle(e.Graphics,new Rectangle(1,2,20,21));
    }
}
internal sealed class ContentTabs : TabControl
{
    protected override void WndProc(ref Message m)
    {
        if(m.Msg==0x1328){m.Result=IntPtr.Zero;return;}
        base.WndProc(ref m);
    }
}
