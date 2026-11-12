// What the Rust side can't reach through cxx-qt-lib: the window's identity, so Desktop Mode's taskbar shows
// Turbine Hub's name and icon instead of a generic one, the keys, mouse and finger of the build's checks, and Qt's
// message handler (see the end of this file).

#include <QtCore/QCoreApplication>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QWheelEvent>
#include <QtGui/QPointingDevice>
#include <QtGui/QWindow>
#include <QtGui/qtestsupport_gui.h>

// With the desktop file's name the taskbar finds Turbine Hub's menu entry; the window icon itself comes from the
// icons the bundle installs, for every place that asks the window and not the entry.
extern "C" void turbinehub_window_identity()
{
    const QString id = QStringLiteral("io.github.ravenor907.TurbineHub");
    QGuiApplication::setDesktopFileName(id);
    QIcon icon;
    for (const int n : {32, 48, 64, 128, 256, 512}) {
        const QString file = QStringLiteral("/app/share/icons/hicolor/%1x%1/apps/%2.png").arg(n).arg(id);
        if (QFile::exists(file))
            icon.addFile(file, QSize(n, n));
    }
    if (icon.isNull())
        icon = QIcon::fromTheme(id);
    if (!icon.isNull())
        QGuiApplication::setWindowIcon(icon);
}

// The build's checks (screenshot runs only): keys and a mouse delivered to the window the way the system delivers
// real ones, so the checks go through the same handlers a hand does and not through a shortcut around them.
namespace {
QWindow *mainWindow()
{
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *w : windows) {
        if (w->objectName() == QLatin1String("turbinehubMain"))
            return w;
    }
    return nullptr;
}

quint64 stamp()
{
    static QElapsedTimer clock;
    if (!clock.isValid())
        clock.start();
    return static_cast<quint64>(clock.elapsed());
}
}

// key: Qt's key code. mods: Qt's keyboard modifiers. repeat: the keyboard repeating a key that is held down.
// Returns 1 when sent, -1 when the window isn't there.
extern "C" int turbinehub_test_key(int key, int mods, int pressed, int repeat)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    QKeyEvent ev(pressed ? QEvent::KeyPress : QEvent::KeyRelease, key, Qt::KeyboardModifiers::fromInt(mods), QString(), repeat != 0);
    QCoreApplication::sendEvent(win, &ev);
    return 1;
}

// kind: 0 move, 1 press, 2 release, 3 wheel. button: Qt's mouse button. wheel: steps, positive scrolls down.
// Returns 1 when sent, -1 when the window isn't there.
extern "C" int turbinehub_test_mouse(int kind, double x, double y, int button, int mods, int wheel)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    const QPointF local(x, y);
    const QPointF global = win->mapToGlobal(local);
    const Qt::KeyboardModifiers held = Qt::KeyboardModifiers::fromInt(mods);
    if (kind == 3) {
        QWheelEvent ev(local, global, QPoint(), QPoint(0, -120 * wheel), Qt::NoButton, held, Qt::NoScrollPhase, false);
        ev.setTimestamp(stamp());
        QCoreApplication::sendEvent(win, &ev);
        return 1;
    }
    const Qt::MouseButton b = kind == 0 ? Qt::NoButton : static_cast<Qt::MouseButton>(button);
    const Qt::MouseButtons down = kind == 1 ? Qt::MouseButtons(b) : Qt::MouseButtons(Qt::NoButton);
    const QEvent::Type type = kind == 1 ? QEvent::MouseButtonPress : kind == 2 ? QEvent::MouseButtonRelease : QEvent::MouseMove;
    QMouseEvent ev(type, local, global, b, down, held);
    ev.setTimestamp(stamp());
    QCoreApplication::sendEvent(win, &ev);
    return 1;
}

// A finger on the screen at a place in the window. kind: 0 down, 1 moved, 2 lifted. It goes in where the system's
// own touches do (Qt's public test support in QtGui), so the window treats it as it treats a real finger.
// Returns 1 when sent, -1 when the window isn't there.
extern "C" int turbinehub_test_touch(int kind, double x, double y)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    static QPointingDevice *screen = QTest::createTouchDevice();
    const QPointF local(x, y);
    const QEventPoint::State state = kind == 0 ? QEventPoint::Pressed : kind == 2 ? QEventPoint::Released : QEventPoint::Updated;
    QList<QEventPoint> points;
    points.append(QEventPoint(1, state, local, win->mapToGlobal(local)));
    qt_handleTouchEvent(win, screen, points);
    return 1;
}

// Qt's and QML's messages into Turbine Hub's own log (app/src/log.rs): warnings and console lines as log lines,
// fatal errors into the crash log. Self-contained: one function, installed once from main.
#include <QtCore/QByteArray>
#include <QtCore/QtGlobal>

namespace {
using LogSink = void (*)(int kind, const char *place, const char *text);
LogSink s_log_sink = nullptr;
QtMessageHandler s_log_previous = nullptr;

void log_handler(QtMsgType type, const QMessageLogContext &ctx, const QString &msg)
{
    if (s_log_sink) {
        const int kind = type == QtDebugMsg ? 0 : type == QtWarningMsg ? 1 : type == QtCriticalMsg ? 2 : type == QtFatalMsg ? 3 : 4;
        QByteArray place;
        if (ctx.file && *ctx.file)
            place = QByteArray(ctx.file) + ':' + QByteArray::number(ctx.line);
        else if (ctx.category && qstrcmp(ctx.category, "default") != 0)
            place = QByteArray(ctx.category);
        const QByteArray text = msg.toUtf8();
        s_log_sink(kind, place.constData(), text.constData());
    }
    // Everything still goes where it went before (the terminal), so the build's checks see QML errors.
    if (s_log_previous)
        s_log_previous(type, ctx, msg);
}
}

extern "C" void turbinehub_install_qt_handler(LogSink sink)
{
    s_log_sink = sink;
    s_log_previous = qInstallMessageHandler(log_handler);
}
