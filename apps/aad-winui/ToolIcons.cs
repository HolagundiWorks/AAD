using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.UI;

namespace Aad.WinUi;

/// <summary>
/// The tool-strip marks, the same 24×24 line drawings as the other window: gray for geometry
/// already there, blue for what the command adds, orange for a picked point.
/// </summary>
internal static class ToolIcons
{
    public static UIElement For(string id, bool dark, bool onFill = false)
    {
        var canvas = new Canvas { Width = 24, Height = 24 };
        var pen = new Pen(canvas, dark, onFill);
        switch (id)
        {
            case "line":
                pen.Accent(4, 20, 20, 4);
                pen.Dot(4, 20);
                pen.Dot(20, 4);
                break;
            case "pline":
                pen.Stroke(false, pen.AccentBrush, (3, 19), (8, 8), (14, 15));
                pen.Arc(17, 11, 5, 220, 380, pen.AccentBrush);
                pen.Dot(3, 19);
                pen.Dot(8, 8);
                pen.Dot(14, 15);
                break;
            case "circle":
                pen.Circle(12, 12, 8, pen.AccentBrush);
                pen.Base(12, 12, 18, 6.5);
                pen.Dot(12, 12);
                break;
            case "arc":
                pen.Arc(12, 16, 9, 10, 170, pen.AccentBrush);
                pen.Dot(3.2, 14.5);
                pen.Dot(12, 7);
                pen.Dot(20.8, 14.5);
                break;
            case "rectang":
                pen.Box(3, 6, 18, 12, pen.AccentBrush);
                pen.Dot(3, 18);
                pen.Dot(21, 6);
                break;
            case "move":
                pen.Accent(12, 3, 12, 21);
                pen.Accent(3, 12, 21, 12);
                pen.Arrow(12, 2, 0, -1);
                pen.Arrow(12, 22, 0, 1);
                pen.Arrow(2, 12, -1, 0);
                pen.Arrow(22, 12, 1, 0);
                break;
            case "copy":
                pen.Box(3, 3, 11, 13, pen.BaseBrush);
                pen.Box(10, 8, 11, 13, pen.AccentBrush);
                break;
            case "rotate":
                pen.Arc(12, 12, 8, 100, 380, pen.AccentBrush);
                pen.Arrow(10.6, 4.1, -1, 0.1);
                pen.Dot(12, 12);
                break;
            case "offset":
                pen.Arc(14, 18, 6, 90, 180, pen.BaseBrush);
                pen.Base(14, 12, 21, 12);
                pen.Base(8, 18, 8, 22);
                pen.Arc(14, 18, 10, 90, 180, pen.AccentBrush);
                pen.Accent(14, 8, 21, 8);
                pen.Accent(4, 18, 4, 22);
                break;
            case "trim":
                pen.Base(8, 2, 8, 22);
                pen.Base(16, 2, 16, 22);
                pen.Base(2, 12, 8, 12);
                pen.Base(16, 12, 22, 12);
                pen.Accent(9.5, 12, 10.5, 12);
                pen.Accent(12, 12, 13, 12);
                pen.Accent(14.5, 12, 15.5, 12);
                pen.Stroke(false, pen.AccentBrush, (10, 17), (12, 15), (14, 17));
                break;
            case "erase":
                pen.Stroke(true, pen.BaseBrush, (4, 16), (13, 7), (19, 13), (12, 20), (8, 20));
                pen.Accent(9, 11, 15, 17);
                pen.Base(8, 20, 20, 20);
                break;
            case "dimlinear":
                pen.Base(4, 18, 4, 5);
                pen.Base(20, 18, 20, 5);
                pen.Accent(4, 8, 20, 8);
                pen.Arrow(4, 8, -1, 0);
                pen.Arrow(20, 8, 1, 0);
                pen.Label("1.5", 12, 14.5, 6, pen.BaseBrush);
                break;
            case "text":
                pen.Label("A", 12, 12.5, 14, pen.BaseBrush);
                break;
            case "hatch":
                pen.Box(3, 3, 18, 18, pen.BaseBrush);
                for (var k = 0; k < 5; k++)
                {
                    var o = k * 4.5;
                    pen.Accent(Math.Min(3 + o, 21), 21, 21, Math.Max(21 - (18 - o), 3));
                }
                break;
            default:
                Mark(pen, id);
                break;
        }
        return new Viewbox { Width = 16, Height = 16, Child = canvas, Stretch = Stretch.Uniform };
    }

    private static void Mark(Pen pen, string id)
    {
        var gold = new SolidColorBrush(Color.FromArgb(255, 230, 180, 60));
        var ice = new SolidColorBrush(Color.FromArgb(255, 159, 212, 255));
        var green = new SolidColorBrush(Color.FromArgb(255, 111, 224, 111));
        var dim = new SolidColorBrush(((SolidColorBrush)pen.BaseBrush).Color) { Opacity = 0.4 };
        switch (id)
        {
            case "bulb":
                pen.FillCircle(12, 10, 6, new SolidColorBrush(Color.FromArgb(255, 245, 208, 59)));
                pen.FillRect(9.5, 15, 5, 4, new SolidColorBrush(Color.FromArgb(255, 176, 180, 186)));
                break;
            case "bulb-off":
                pen.FillCircle(12, 10, 6, new SolidColorBrush(Color.FromArgb(255, 112, 120, 134)));
                pen.FillRect(9.5, 15, 5, 4, dim);
                break;
            case "sun":
                pen.FillCircle(12, 12, 4, gold);
                for (var i = 0; i < 8; i++)
                {
                    var angle = i * 45.0 * Math.PI / 180.0;
                    pen.Line(12 + 6 * Math.Cos(angle), 12 + 6 * Math.Sin(angle), 12 + 9 * Math.Cos(angle), 12 + 9 * Math.Sin(angle), gold, 1.5);
                }
                break;
            case "snowflake":
                for (var i = 0; i < 3; i++)
                {
                    var angle = i * 60.0 * Math.PI / 180.0;
                    pen.Line(12 - 8 * Math.Cos(angle), 12 - 8 * Math.Sin(angle), 12 + 8 * Math.Cos(angle), 12 + 8 * Math.Sin(angle), ice, 1.5);
                }
                break;
            case "lock":
                pen.FillRect(6, 11, 12, 9, gold);
                pen.Arc(12, 11, 4, 0, 180, gold);
                break;
            case "unlock":
                pen.FillRect(6, 11, 12, 9, gold);
                pen.Arc(15, 9, 4, 0, 180, gold);
                break;
            case "print":
                pen.Box(4, 9, 16, 8, pen.BaseBrush);
                pen.Box(7, 4, 10, 5, pen.BaseBrush);
                pen.Box(7, 14, 10, 7, pen.BaseBrush);
                break;
            case "print-off":
                pen.Box(4, 9, 16, 8, dim);
                pen.Box(7, 4, 10, 5, dim);
                pen.Box(7, 14, 10, 7, dim);
                break;
            case "check":
                pen.Line(5, 12, 10, 18, green, 2);
                pen.Line(10, 18, 19, 6, green, 2);
                break;
            case "layers":
                for (var k = 0; k < 3; k++)
                {
                    var y = 5.0 + k * 5;
                    pen.Stroke(true, k == 0 ? pen.AccentBrush : pen.BaseBrush, (3, y + 2), (12, y - 2), (21, y + 2), (12, y + 6));
                }
                break;
            case "layer-new":
                Mark(pen, "layers");
                pen.Line(18, 16, 18, 22, pen.AccentBrush, 1.5);
                pen.Line(15, 19, 21, 19, pen.AccentBrush, 1.5);
                break;
            case "layer-frozen":
                Mark(pen, "layers");
                pen.FillCircle(18, 18, 3, ice);
                break;
            case "undo":
                pen.Arc(12, 13, 7, 150, -40, pen.BaseBrush);
                pen.Line(5, 9, 2, 6.5, pen.BaseBrush, 1.5);
                pen.Line(5, 9, 8, 7, pen.BaseBrush, 1.5);
                break;
            case "redo":
                pen.Arc(12, 13, 7, 30, 220, pen.BaseBrush);
                pen.Line(19, 9, 22, 6.5, pen.BaseBrush, 1.5);
                pen.Line(19, 9, 16, 7, pen.BaseBrush, 1.5);
                break;
            case "refresh":
                pen.Arc(12, 12, 7, 40, 300, pen.BaseBrush);
                pen.Line(17.5, 7.5, 19, 4, pen.BaseBrush, 1.5);
                pen.Line(17.5, 7.5, 14.5, 6, pen.BaseBrush, 1.5);
                break;
            case "gear":
                pen.Circle(12, 12, 3.5, pen.BaseBrush);
                for (var i = 0; i < 8; i++)
                {
                    var angle = i * 45.0 * Math.PI / 180.0;
                    pen.Line(12 + 6 * Math.Cos(angle), 12 + 6 * Math.Sin(angle), 12 + 9 * Math.Cos(angle), 12 + 9 * Math.Sin(angle), pen.BaseBrush, 2.2);
                }
                break;
            case "delete":
                pen.Line(6, 6, 18, 18, pen.BaseBrush, 1.8);
                pen.Line(18, 6, 6, 18, pen.BaseBrush, 1.8);
                break;
            case "plus":
                pen.Line(12, 4, 12, 20, pen.BaseBrush, 1.8);
                pen.Line(4, 12, 20, 12, pen.BaseBrush, 1.8);
                break;
            case "osnap":
                pen.Box(7, 7, 10, 10, pen.BaseBrush);
                pen.Base(12, 2, 12, 7);
                pen.Base(12, 17, 12, 22);
                pen.Base(2, 12, 7, 12);
                pen.Base(17, 12, 22, 12);
                break;
            case "ortho":
                pen.Stroke(false, pen.BaseBrush, (4, 3), (4, 20), (21, 20));
                break;
            case "polar":
                pen.Base(4, 20, 21, 20);
                pen.Base(4, 20, 18, 6);
                pen.Arc(4, 20, 9, 0, 45, pen.BaseBrush);
                pen.Circle(4, 20, 1.2, pen.BaseBrush);
                break;
            case "otrack":
                pen.Box(9, 9, 6, 6, pen.BaseBrush);
                foreach (var x in new[] { 2.0, 5, 17, 20 })
                {
                    pen.Base(x, 12, x + 1.5, 12);
                }
                foreach (var y in new[] { 2.0, 5, 17, 20 })
                {
                    pen.Base(12, y, 12, y + 1.5);
                }
                break;
            case "dyn":
                pen.Box(3, 6, 12, 7, pen.BaseBrush);
                pen.Base(6, 9.5, 12, 9.5);
                pen.Base(17, 13, 21, 21);
                pen.Base(17, 13, 17, 21);
                break;
            case "lineweight":
                pen.Line(3, 6, 21, 6, pen.BaseBrush, 1);
                pen.Line(3, 12, 21, 12, pen.BaseBrush, 2);
                pen.Line(3, 18, 21, 18, pen.BaseBrush, 3.5);
                break;
            case "transparency":
                pen.FillRect(3, 3, 12, 12, new SolidColorBrush(((SolidColorBrush)pen.BaseBrush).Color) { Opacity = 0.7 });
                pen.FillRect(9, 9, 12, 12, new SolidColorBrush(((SolidColorBrush)pen.BaseBrush).Color) { Opacity = 0.35 });
                break;
            case "quickprops":
                pen.Box(3, 4, 15, 13, pen.BaseBrush);
                pen.Base(3, 7, 18, 7);
                pen.Base(5, 10, 9, 10);
                pen.Accent(11, 10, 16, 10);
                pen.Base(5, 13, 9, 13);
                pen.Accent(11, 13, 16, 13);
                break;
            case "workspace":
                pen.Box(3, 4, 18, 16, pen.BaseBrush);
                pen.Base(9, 4, 9, 20);
                pen.Base(9, 9, 21, 9);
                break;
            case "model":
                pen.Box(4, 7, 16, 10, pen.AccentBrush);
                pen.Base(3, 19, 21, 19);
                break;
            case "paper":
                pen.Box(6, 2, 12, 18, pen.BaseBrush);
                pen.Box(8, 6, 8, 8, pen.AccentBrush);
                break;
            case "moon":
                pen.Arc(11, 12, 7, 70, 290, pen.BaseBrush);
                pen.Arc(14, 12, 5.5, 100, 260, pen.BaseBrush);
                break;
            case "pdf":
                pen.Box(4, 3, 12, 16, pen.BaseBrush);
                pen.Stroke(false, pen.BaseBrush, (12, 3), (16, 7), (12, 7));
                pen.Accent(7, 12, 13, 12);
                pen.Accent(7, 15, 12, 15);
                break;
            case "pin":
                pen.Circle(12, 7, 3.2, pen.BaseBrush);
                pen.Line(12, 10, 12, 16, pen.BaseBrush, 1.6);
                pen.Stroke(true, pen.BaseBrush, (8.5, 16), (15.5, 16), (12, 21));
                break;
            case "unpin":
                pen.Circle(12, 7, 3.2, pen.BaseBrush);
                pen.Line(12, 10, 12, 16, pen.BaseBrush, 1.6);
                pen.Stroke(true, pen.BaseBrush, (8.5, 16), (15.5, 16), (12, 21));
                pen.Line(5, 19, 19, 5, pen.AccentBrush, 1.6);
                break;
            case "float":
                pen.Box(2, 8, 12, 13, pen.BaseBrush);
                pen.Box(9, 3, 12, 11, pen.AccentBrush);
                break;
            case "dock":
                pen.Box(14, 3, 7, 18, pen.BaseBrush);
                pen.Box(2, 7, 11, 10, pen.AccentBrush);
                break;
            case "undock":
                pen.Box(14, 3, 7, 18, pen.BaseBrush);
                pen.Box(2, 4, 11, 10, pen.AccentBrush);
                pen.Line(8, 16, 12, 20, pen.AccentBrush, 1.5);
                break;
        }
    }

    private sealed class Pen
    {
        private readonly Canvas _canvas;
        public Brush BaseBrush { get; }
        public Brush AccentBrush { get; }
        private readonly Brush _point;

        public Pen(Canvas canvas, bool dark, bool onFill = false)
        {
            _canvas = canvas;
            // A pressed-in toggle is filled with the accent (light neutral in the dark theme, ink in the
            // light one), so its strokes take the opposite tone to stay visible.
            BaseBrush = new SolidColorBrush(onFill
                ? (dark ? Color.FromArgb(255, 23, 24, 27) : Color.FromArgb(255, 246, 245, 241))
                : (dark ? Color.FromArgb(255, 214, 214, 214) : Color.FromArgb(255, 45, 45, 45)));
            AccentBrush = new SolidColorBrush(onFill && dark ? Color.FromArgb(255, 27, 111, 214) : Color.FromArgb(255, 40, 132, 220));
            _point = new SolidColorBrush(Color.FromArgb(255, 224, 138, 28));
        }

        public void Base(double x1, double y1, double x2, double y2) => Segment(x1, y1, x2, y2, BaseBrush);

        public void Accent(double x1, double y1, double x2, double y2) => Segment(x1, y1, x2, y2, AccentBrush);

        public void Dot(double x, double y)
        {
            var mark = new Ellipse { Width = 3.2, Height = 3.2, Fill = _point };
            Canvas.SetLeft(mark, x - 1.6);
            Canvas.SetTop(mark, y - 1.6);
            _canvas.Children.Add(mark);
        }

        public void FillCircle(double x, double y, double radius, Brush brush)
        {
            var mark = new Ellipse { Width = radius * 2, Height = radius * 2, Fill = brush };
            Canvas.SetLeft(mark, x - radius);
            Canvas.SetTop(mark, y - radius);
            _canvas.Children.Add(mark);
        }

        public void FillRect(double x, double y, double width, double height, Brush brush)
        {
            var mark = new Rectangle { Width = width, Height = height, Fill = brush };
            Canvas.SetLeft(mark, x);
            Canvas.SetTop(mark, y);
            _canvas.Children.Add(mark);
        }

        public void Line(double x1, double y1, double x2, double y2, Brush brush, double thickness)
        {
            _canvas.Children.Add(new Line
            {
                X1 = x1,
                Y1 = y1,
                X2 = x2,
                Y2 = y2,
                Stroke = brush,
                StrokeThickness = thickness,
                StrokeStartLineCap = PenLineCap.Round,
                StrokeEndLineCap = PenLineCap.Round,
            });
        }

        public void Circle(double x, double y, double radius, Brush brush)
        {
            var mark = new Ellipse
            {
                Width = radius * 2,
                Height = radius * 2,
                Stroke = brush,
                StrokeThickness = 1.5,
            };
            Canvas.SetLeft(mark, x - radius);
            Canvas.SetTop(mark, y - radius);
            _canvas.Children.Add(mark);
        }

        public void Box(double x, double y, double width, double height, Brush brush) =>
            Stroke(true, brush, (x, y), (x + width, y), (x + width, y + height), (x, y + height));

        public void Stroke(bool closed, Brush brush, params (double X, double Y)[] points)
        {
            var figure = new PathFigure { IsClosed = closed };
            figure.StartPoint = new Point(points[0].X, points[0].Y);
            for (var i = 1; i < points.Length; i++)
            {
                figure.Segments.Add(new LineSegment { Point = new Point(points[i].X, points[i].Y) });
            }
            var geometry = new PathGeometry();
            geometry.Figures.Add(figure);
            _canvas.Children.Add(new Microsoft.UI.Xaml.Shapes.Path
            {
                Data = geometry,
                Stroke = brush,
                StrokeThickness = 1.5,
                StrokeLineJoin = PenLineJoin.Round,
                StrokeStartLineCap = PenLineCap.Round,
                StrokeEndLineCap = PenLineCap.Round,
            });
        }

        public void Arc(double x, double y, double radius, double start, double end, Brush brush)
        {
            var steps = Math.Max(2, (int)Math.Ceiling(Math.Abs(end - start) / 10.0));
            var points = new (double X, double Y)[steps + 1];
            for (var i = 0; i <= steps; i++)
            {
                var angle = (start + (end - start) * i / steps) * Math.PI / 180.0;
                points[i] = (x + radius * Math.Cos(angle), y - radius * Math.Sin(angle));
            }
            Stroke(false, brush, points);
        }

        public void Arrow(double x, double y, double dx, double dy)
        {
            var length = Math.Max(Math.Sqrt(dx * dx + dy * dy), 1e-3);
            var ux = dx / length;
            var uy = dy / length;
            var nx = -uy;
            var ny = ux;
            var head = new Polygon { Fill = AccentBrush };
            head.Points = new PointCollection
            {
                new Point(x, y),
                new Point(x - ux * 3 + nx * 1.4, y - uy * 3 + ny * 1.4),
                new Point(x - ux * 3 - nx * 1.4, y - uy * 3 - ny * 1.4),
            };
            _canvas.Children.Add(head);
        }

        public void Label(string text, double x, double y, double size, Brush brush)
        {
            var block = new TextBlock { Text = text, FontSize = size, Foreground = brush };
            // A mark drawn inside an icon: the button already carries the name.
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAccessibilityView(block, Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Raw);
            Canvas.SetLeft(block, x - size * text.Length * 0.32);
            Canvas.SetTop(block, y - size * 0.7);
            _canvas.Children.Add(block);
        }

        private void Segment(double x1, double y1, double x2, double y2, Brush brush) => Line(x1, y1, x2, y2, brush, 1.5);
    }
}

/// <summary>A drawn mark that follows the window theme.</summary>
public sealed class DrawnIcon : ContentControl
{
    public static readonly DependencyProperty KindProperty = DependencyProperty.Register(
        nameof(Kind),
        typeof(string),
        typeof(DrawnIcon),
        new PropertyMetadata("", (sender, _) => ((DrawnIcon)sender).Redraw()));

    public DrawnIcon()
    {
        Width = 16;
        Height = 16;
        IsTabStop = false;
        IsHitTestVisible = false;
        HorizontalContentAlignment = HorizontalAlignment.Center;
        VerticalContentAlignment = VerticalAlignment.Center;
        Loaded += (_, _) =>
        {
            HookToggle();
            Redraw();
        };
        ActualThemeChanged += (_, _) => Redraw();
    }

    private Microsoft.UI.Xaml.Controls.Primitives.ToggleButton? _toggle;

    /// <summary>Redraw when the toggle button that holds this mark is pressed in or released.</summary>
    private void HookToggle()
    {
        DependencyObject? node = this;
        while (node != null && node is not Microsoft.UI.Xaml.Controls.Primitives.ToggleButton)
        {
            node = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetParent(node);
        }
        if (node is Microsoft.UI.Xaml.Controls.Primitives.ToggleButton toggle && !ReferenceEquals(toggle, _toggle))
        {
            _toggle = toggle;
            toggle.Checked += (_, _) => Redraw();
            toggle.Unchecked += (_, _) => Redraw();
            toggle.Indeterminate += (_, _) => Redraw();
        }
    }

    public string Kind
    {
        get => (string)GetValue(KindProperty);
        set => SetValue(KindProperty, value);
    }

    private void Redraw()
    {
        Content = string.IsNullOrEmpty(Kind) ? null : ToolIcons.For(Kind, ActualTheme != ElementTheme.Light, _toggle?.IsChecked == true);
    }
}
