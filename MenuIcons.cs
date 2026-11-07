using System.Drawing.Drawing2D;
namespace SlimBrowser;
internal static class MenuIcons
{
    public static Bitmap Create(string name)
    {
        var image=new Bitmap(20,20);using var g=Graphics.FromImage(image);g.SmoothingMode=SmoothingMode.AntiAlias;
        using var p=new Pen(ChromeTheme.Ink,1.4f){StartCap=LineCap.Round,EndCap=LineCap.Round,LineJoin=LineJoin.Round};
        if(name.StartsWith("Загрузки")){g.DrawLine(p,10,2,10,13);g.DrawLines(p,[new(6,9),new(10,13),new(14,9)]);g.DrawLines(p,[new(3,13),new(3,17),new(17,17),new(17,13)]);}
        else if(name.StartsWith("Пароли")){g.DrawEllipse(p,2,3,8,8);g.DrawLine(p,9,10,17,18);g.DrawLine(p,13,14,16,11);}
        else if(name.StartsWith("Журнал")){g.DrawArc(p,3,3,14,14,-80,300);g.DrawLine(p,10,6,10,10);g.DrawLine(p,10,10,14,12);g.DrawLines(p,[new(1,3),new(2,8),new(7,7)]);}
        else if(name.Contains("заклад")||name=="Закладки"){g.DrawLines(p,[new(5,3),new(15,3),new(15,18),new(10,14),new(5,18),new(5,3)]);}
        else if(name.StartsWith("Настройки")||name.StartsWith("Расширения")){g.DrawLine(p,3,6,17,6);g.DrawLine(p,3,14,17,14);g.DrawEllipse(p,6,3,6,6);g.DrawEllipse(p,10,11,6,6);}
        else{g.DrawRectangle(p,3,4,14,12);g.DrawLine(p,3,8,17,8);}
        return image;
    }
}
