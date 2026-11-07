using System.Drawing.Drawing2D;

namespace SlimBrowser;

// Keep alpha in the DWM redirection surface. Form.Opacity would also fade sites and text.
internal static class MaterialPaint
{
    public static void Background(Graphics graphics, Control control, Color? fill = null)
    {
        graphics.CompositingMode = CompositingMode.SourceCopy;
        graphics.Clear(fill ?? ChromeTheme.Material);
        graphics.CompositingMode = CompositingMode.SourceOver;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
    }
    public static void Text(Graphics graphics, string text, Font font, Rectangle bounds, Color color, bool centered = false)
    {
        using var brush = new SolidBrush(color);
        using var format = new StringFormat { LineAlignment = StringAlignment.Center, Alignment = centered ? StringAlignment.Center : StringAlignment.Near, Trimming = StringTrimming.EllipsisCharacter, FormatFlags = StringFormatFlags.NoWrap };
        graphics.DrawString(text, font, brush, bounds, format);
    }
}

internal class MaterialPanel : Panel
{
    public MaterialPanel() { DoubleBuffered = true; ResizeRedraw = true; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
}
internal sealed class MaterialFlowPanel : FlowLayoutPanel
{
    public MaterialFlowPanel() { DoubleBuffered = true; ResizeRedraw = true; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
}
internal sealed class MaterialTablePanel : TableLayoutPanel
{
    public MaterialTablePanel() { DoubleBuffered = true; ResizeRedraw = true; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
}
internal sealed class MaterialTabPage : TabPage
{
    public MaterialTabPage(string title) : base(title) { DoubleBuffered = true; UseVisualStyleBackColor = false; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
}
internal sealed class MaterialLabel : Label
{
    public MaterialLabel() { DoubleBuffered = true; UseCompatibleTextRendering = true; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
    protected override void OnPaint(PaintEventArgs e)
    {
        using var brush = new SolidBrush(ForeColor);
        using var format = new StringFormat { LineAlignment = AutoSize ? StringAlignment.Near : StringAlignment.Center };
        if (TextAlign == ContentAlignment.MiddleCenter) format.Alignment = StringAlignment.Center;
        if (TextAlign == ContentAlignment.MiddleRight) format.Alignment = StringAlignment.Far;
        e.Graphics.DrawString(Text, Font, brush, new Rectangle(Padding.Left, Padding.Top, Width - Padding.Horizontal, Height - Padding.Vertical), format);
    }
}
internal sealed class MaterialToggle : Control
{
    bool isChecked;
    [System.ComponentModel.DefaultValue(false)] public bool Checked { get=>isChecked; set {if(isChecked==value)return;isChecked=value;Invalidate();AccessibilityNotifyClients(AccessibleEvents.StateChange,-1);CheckedChanged?.Invoke(this,EventArgs.Empty);} }
    public event EventHandler? CheckedChanged;
    public MaterialToggle() { Width = 610; Height = 38; Cursor = Cursors.Hand; DoubleBuffered = true; TabStop=true;AccessibleRole=AccessibleRole.CheckButton; }
    protected override void OnClick(EventArgs e){Checked=!Checked;base.OnClick(e);}
    protected override void OnMouseDown(MouseEventArgs e){Focus();base.OnMouseDown(e);}
    protected override void OnKeyDown(KeyEventArgs e){base.OnKeyDown(e);if(e.KeyCode==Keys.Space){OnClick(EventArgs.Empty);e.Handled=e.SuppressKeyPress=true;}}
    protected override void OnGotFocus(EventArgs e){base.OnGotFocus(e);Invalidate();}
    protected override void OnLostFocus(EventArgs e){base.OnLostFocus(e);Invalidate();}
    protected override AccessibleObject CreateAccessibilityInstance()=>new ToggleAccessibility(this);
    sealed class ToggleAccessibility(MaterialToggle owner):ControlAccessibleObject(owner)
    {
        public override AccessibleStates State=>base.State|(owner.Checked?AccessibleStates.Checked:AccessibleStates.None);
        public override string? DefaultAction=>"Переключить";
        public override void DoDefaultAction(){if(owner.Enabled)owner.OnClick(EventArgs.Empty);}
    }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        using var track = ChromeTheme.Round(new Rectangle(1, 7, 38, 23), 11);
        using var fill = new SolidBrush(Checked ? ChromeTheme.Accent : ChromeTheme.Field);
        using var edge = new Pen(ChromeTheme.Border);
        g.FillPath(fill, track); g.DrawPath(edge, track);
        using var thumb = new SolidBrush(Checked ? Color.White : ChromeTheme.Ink);
        g.FillEllipse(thumb, Checked ? 19 : 4, 10, 17, 17);
        MaterialPaint.Text(g, Text, Font, new Rectangle(52, 0, Width - 54, Height), ChromeTheme.Ink);
        if (Focused && ShowFocusCues) ControlPaint.DrawFocusRectangle(g, new Rectangle(0, 3, Width - 1, Height - 6));
    }
}

internal sealed class MaterialSlider : Control
{
    int value;
    [System.ComponentModel.DefaultValue(0)] public int Value { get => value; set { int next = Math.Clamp(value, 0, 100); if (this.value == next) return; this.value = next; Invalidate(); AccessibilityNotifyClients(AccessibleEvents.ValueChange, -1); ValueChanged?.Invoke(this, EventArgs.Empty); } }
    public event EventHandler? ValueChanged;
    public event EventHandler? ValueCommitted;
    public MaterialSlider() { Size = new Size(590, 40); TabStop = true; Cursor = Cursors.Hand; DoubleBuffered = true; AccessibleName = "Прозрачность интерфейса"; AccessibleRole = AccessibleRole.Slider; }
    protected override void OnPaintBackground(PaintEventArgs e) => MaterialPaint.Background(e.Graphics, this);
    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics; int end = Width - 14; float x = 14 + (end - 14) * Value / 100f;
        using var track = new Pen(ChromeTheme.Border, 5) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        using var active = new Pen(ChromeTheme.Accent, 5) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        g.DrawLine(track, 14, Height / 2, end, Height / 2);
        if (Value > 0) g.DrawLine(active, 14, Height / 2, x, Height / 2);
        using var shadow = new SolidBrush(Color.FromArgb(28, 0, 0, 0)); g.FillEllipse(shadow, x - 12, Height / 2 - 10, 24, 24);
        using var thumb = new SolidBrush(Color.White); g.FillEllipse(thumb, x - 11, Height / 2 - 11, 22, 22);
        using var edge = new Pen(Focused ? ChromeTheme.Accent : ChromeTheme.Border, Focused ? 2 : 1); g.DrawEllipse(edge, x - 11, Height / 2 - 11, 22, 22);
    }
    void Track(int x) => Value = (int)Math.Round((x - 14) * 100.0 / Math.Max(1, Width - 28));
    protected override void OnMouseDown(MouseEventArgs e) { base.OnMouseDown(e); if (e.Button != MouseButtons.Left) return; Focus(); Capture = true; Track(e.X); }
    protected override void OnMouseMove(MouseEventArgs e) { base.OnMouseMove(e); if (Capture) Track(e.X); }
    protected override void OnMouseUp(MouseEventArgs e) { base.OnMouseUp(e); if (e.Button != MouseButtons.Left || !Capture) return; Track(e.X); Capture = false; ValueCommitted?.Invoke(this, EventArgs.Empty); }
    protected override bool IsInputKey(Keys keyData) => (keyData & Keys.KeyCode) is Keys.Left or Keys.Right or Keys.Up or Keys.Down or Keys.Home or Keys.End or Keys.PageUp or Keys.PageDown || base.IsInputKey(keyData);
    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        int? next = e.KeyCode switch { Keys.Left or Keys.Down => Value - 1, Keys.Right or Keys.Up => Value + 1, Keys.PageDown => Value - 10, Keys.PageUp => Value + 10, Keys.Home => 0, Keys.End => 100, _ => null };
        if (next is null) return; Value = next.Value; e.Handled = e.SuppressKeyPress = true; ValueCommitted?.Invoke(this, EventArgs.Empty);
    }
    protected override void OnGotFocus(EventArgs e) { base.OnGotFocus(e); Invalidate(); }
    protected override void OnLostFocus(EventArgs e) { base.OnLostFocus(e); Invalidate(); }
    protected override AccessibleObject CreateAccessibilityInstance() => new SliderAccessibility(this);
    sealed class SliderAccessibility(MaterialSlider owner) : ControlAccessibleObject(owner)
    {
        public override string? Value { get => owner.Value + "%"; set { if (int.TryParse(value?.TrimEnd('%'), out var parsed)) { owner.Value = parsed; owner.ValueCommitted?.Invoke(owner, EventArgs.Empty); } } }
    }
}
