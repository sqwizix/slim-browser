using System.Runtime.InteropServices;
namespace SlimBrowser;

// Native EDIT painting uses GDI without an alpha channel. On a DWM glass frame,
// explicitly mark the finished pixels opaque while keeping native editing/caret.
internal sealed class GlassTextBox : TextBox
{
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct PaintStruct
    {
        public IntPtr Hdc; public int Erase; public Rect Rect; public int Restore,IncUpdate;
        [MarshalAs(UnmanagedType.ByValArray,SizeConst=32)] public byte[] Reserved;
    }
    [DllImport("user32.dll")] static extern IntPtr BeginPaint(IntPtr hwnd,out PaintStruct paint);
    [DllImport("user32.dll")] static extern bool EndPaint(IntPtr hwnd,ref PaintStruct paint);
    [DllImport("uxtheme.dll")] static extern int BufferedPaintInit();
    [DllImport("uxtheme.dll")] static extern IntPtr BeginBufferedPaint(IntPtr dc,ref Rect rect,int format,IntPtr parameters,out IntPtr bufferedDc);
    [DllImport("uxtheme.dll")] static extern int BufferedPaintSetAlpha(IntPtr buffer,IntPtr rect,byte alpha);
    [DllImport("uxtheme.dll")] static extern int EndBufferedPaint(IntPtr buffer,bool update);
    [DllImport("uxtheme.dll")] static extern int GetBufferedPaintBits(IntPtr buffer,out IntPtr bits,out int rowWidth);
    static GlassTextBox(){BufferedPaintInit();}
    protected override void WndProc(ref Message m)
    {
        if(m.Msg!=0xF || !ChromeTheme.Glass)
        {
            base.WndProc(ref m);
            if(ChromeTheme.Glass && m.Msg is 0x7 or 0x8 or 0xC or 0x100 or 0x101 or 0x102 or 0x200 or 0x201 or 0x202 or 0x300 or 0x302 or 0x303 or 0x304 or 0xB1)Invalidate();
            return;
        }
        var dc=BeginPaint(Handle,out var paint);
        try
        {
            var rect=new Rect{Right=ClientSize.Width,Bottom=ClientSize.Height};
            var buffer=BeginBufferedPaint(dc,ref rect,2,IntPtr.Zero,out var target);
            if(buffer==IntPtr.Zero)
            {
                var direct=Message.Create(Handle,0x318,dc,(IntPtr)12);base.WndProc(ref direct);return;
            }
            try
            {
                using(var g=Graphics.FromHdc(target))g.Clear(BackColor);
                if(TextLength>0){var print=Message.Create(Handle,0x318,target,(IntPtr)12);base.WndProc(ref print);}
                BufferedPaintSetAlpha(buffer,IntPtr.Zero,255);
                // Native EDIT erases its rectangular background. Restore material alpha only
                // for those background pixels, keeping glyphs, selection and caret opaque.
                if(GetBufferedPaintBits(buffer,out var bits,out var rowWidth)==0 && bits!=IntPtr.Zero)
                {
                    var tint=ChromeTheme.GlassField;
                    var ink=TextLength==0?(ChromeTheme.Dark?Color.FromArgb(165,168,178):SystemColors.GrayText):ForeColor;
                    int background=BackColor.ToArgb()&0xFFFFFF;
                    int tinted=unchecked((int)((uint)tint.A<<24))|(tint.R*tint.A/255<<16)|(tint.G*tint.A/255<<8)|tint.B*tint.A/255;
                    var row=new int[ClientSize.Width];
                    for(int y=0;y<ClientSize.Height;y++)
                    {
                        var address=IntPtr.Add(bits,y*rowWidth*4);Marshal.Copy(address,row,0,row.Length);
                        for(int x=0;x<row.Length;x++)
                        {
                            if((row[x]&0xFFFFFF)==background){row[x]=tinted;continue;}
                            int pixel=row[x],r=(pixel>>16)&255,g=(pixel>>8)&255,b=pixel&255;
                            double coverage=0;
                            if(BackColor.R!=ink.R)coverage=Math.Max(coverage,(r-BackColor.R)/(double)(ink.R-BackColor.R));
                            if(BackColor.G!=ink.G)coverage=Math.Max(coverage,(g-BackColor.G)/(double)(ink.G-BackColor.G));
                            if(BackColor.B!=ink.B)coverage=Math.Max(coverage,(b-BackColor.B)/(double)(ink.B-BackColor.B));
                            coverage=Math.Clamp(coverage,0,1);
                            // Preserve native selection colors; reconstruct antialiased text
                            // over translucent tint instead of leaving an opaque white halo.
                            if(Math.Abs(r-(BackColor.R+(ink.R-BackColor.R)*coverage))>40 || Math.Abs(g-(BackColor.G+(ink.G-BackColor.G)*coverage))>40 || Math.Abs(b-(BackColor.B+(ink.B-BackColor.B)*coverage))>40)continue;
                            int alpha=(int)Math.Round(tint.A+(255-tint.A)*coverage);
                            double removal=(1-tint.A/255.0)*(1-coverage);
                            r=Math.Clamp((int)Math.Round(r-BackColor.R*removal),0,alpha);
                            g=Math.Clamp((int)Math.Round(g-BackColor.G*removal),0,alpha);
                            b=Math.Clamp((int)Math.Round(b-BackColor.B*removal),0,alpha);
                            row[x]=unchecked((int)((uint)alpha<<24))|(r<<16)|(g<<8)|b;
                        }
                        Marshal.Copy(row,0,address,row.Length);
                    }
                }
                if(TextLength==0 && !string.IsNullOrEmpty(PlaceholderText))
                {
                    using var g=Graphics.FromHdc(target);g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
                    using var ink=new SolidBrush(ChromeTheme.Dark?Color.FromArgb(165,168,178):Color.FromArgb(83,86,94));
                    using var format=new StringFormat(StringFormat.GenericTypographic){FormatFlags=StringFormatFlags.NoWrap};
                    g.DrawString(PlaceholderText,Font,ink,ClientRectangle,format);
                }
            }
            finally {EndBufferedPaint(buffer,true);}
        }
        finally {EndPaint(Handle,ref paint);}
        m.Result=IntPtr.Zero;
    }
}
