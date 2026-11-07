namespace SlimBrowser;
internal sealed class SoftButton : Button
{
    [System.ComponentModel.DefaultValue(false)] public bool Selected {get;set;}
    [System.ComponentModel.DefaultValue(false)] public bool NavigationItem {get;set;}
    bool hover,down;
    public SoftButton(){FlatStyle=FlatStyle.Flat;FlatAppearance.BorderSize=0;AutoSize=true;Padding=new Padding(13,7,13,7);Cursor=Cursors.Hand;}
    protected override void OnMouseEnter(EventArgs e){hover=true;Invalidate();base.OnMouseEnter(e);}
    protected override void OnMouseLeave(EventArgs e){hover=down=false;Invalidate();base.OnMouseLeave(e);}
    protected override void OnMouseDown(MouseEventArgs e){down=true;Invalidate();base.OnMouseDown(e);}
    protected override void OnMouseUp(MouseEventArgs e){down=false;Invalidate();base.OnMouseUp(e);}
    protected override void OnPaint(PaintEventArgs e)
    {
        MaterialPaint.Background(e.Graphics,this);
        using var shape=ChromeTheme.Round(new Rectangle(0,0,Width-1,Height-1),10);
        using var fill=new SolidBrush(Selected?Color.FromArgb(ChromeTheme.UseGlass?135:255,ChromeTheme.Field):ChromeTheme.GlassField);using var pen=new Pen(Focused&&ShowFocusCues?ChromeTheme.Accent:Color.FromArgb(70,ChromeTheme.Border));
        if(!NavigationItem||Selected||hover){e.Graphics.FillPath(fill,shape);if(!NavigationItem)e.Graphics.DrawPath(pen,shape);}
        if(hover||down){using var tint=new SolidBrush(Color.FromArgb(down?45:20,ChromeTheme.Dark?Color.White:Color.Black));e.Graphics.FillPath(tint,shape);}
        if(NavigationItem&&Focused&&ShowFocusCues)e.Graphics.DrawPath(pen,shape);
        MaterialPaint.Text(e.Graphics,Text,Font,new Rectangle(NavigationItem?12:0,0,Width-(NavigationItem?24:0),Height),Enabled?ChromeTheme.Ink:SystemColors.GrayText,!NavigationItem);
    }
}
internal sealed class ThemeChoice : RadioButton
{
    public ThemeChoice(){Appearance=Appearance.Button;AutoSize=false;Size=new Size(130,44);Cursor=Cursors.Hand;}
    protected override void OnPaint(PaintEventArgs e)
    {
        MaterialPaint.Background(e.Graphics,this);
        using var shape=ChromeTheme.Round(new Rectangle(1,1,Width-3,Height-3),12);
        using var fill=new SolidBrush(Checked?ChromeTheme.Accent:ChromeTheme.GlassField);using var pen=new Pen(Focused&&ShowFocusCues?Color.FromArgb(120,168,235):ChromeTheme.Border);
        e.Graphics.FillPath(fill,shape);e.Graphics.DrawPath(pen,shape);
        MaterialPaint.Text(e.Graphics,Text,Font,ClientRectangle,Checked?Color.White:ChromeTheme.Ink,true);
    }
}
