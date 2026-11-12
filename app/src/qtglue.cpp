// What the Rust side can't reach through cxx-qt-lib: the window's identity, so Desktop Mode's taskbar shows
// Turbine Hub's name and icon instead of a generic one, the keys, mouse and finger of the build's checks, and Qt's
// message handler (see the end of this file).

#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QElapsedTimer>
#include <QtCore/QFile>
#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusUnixFileDescriptor>
#include <QtGui/QAccessible>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QKeyEvent>
#include <QtGui/QMouseEvent>
#include <QtGui/QWheelEvent>
#include <QtGui/QPointingDevice>
#include <QtGui/QWindow>
#include <QtGui/qtestsupport_gui.h>
#include <cstdio>
#include <fcntl.h>
#include <unistd.h>

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

// Files, Copy as path: text onto the clipboard. Returns 1 when there is a clipboard to put it on.
extern "C" int turbinehub_copy_text(const char *text)
{
    QClipboard *board = QGuiApplication::clipboard();
    if (!board)
        return 0;
    board->setText(QString::fromUtf8(text));
    return 1;
}

// Files, Open with (Desktop Mode): hands a file to the desktop's own portal and asks it to show its list of apps
// (org.freedesktop.portal.OpenURI, OpenFile with "ask"). The file is passed as an open file, the way the portal
// wants it from a sandboxed app; the app that is picked gets that one file and nothing else. Returns 1 when the
// request was sent, 0 when the file can't be opened or there is no session bus (Game Mode has no such list).
extern "C" int turbinehub_open_with(const char *path)
{
    const int fd = ::open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return 0;
    QDBusMessage call = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.portal.Desktop"), QStringLiteral("/org/freedesktop/portal/desktop"), QStringLiteral("org.freedesktop.portal.OpenURI"), QStringLiteral("OpenFile"));
    QVariantMap options;
    options.insert(QStringLiteral("ask"), true);
    call << QString() << QVariant::fromValue(QDBusUnixFileDescriptor(fd)) << options;
    const bool sent = QDBusConnection::sessionBus().isConnected() && QDBusConnection::sessionBus().send(call);
    ::close(fd);
    return sent ? 1 : 0;
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

// What a screen reader is told about the window, asked the way a screen reader asks (Qt's accessibility tree, the
// one AT-SPI hands to Orca), so the checks measure what is really said and not what the QML meant to say.
namespace {
struct Spoken {
    int controls = 0;   // things with a control's role (button, check box, slider, list item, tab, ...)
    int nameless = 0;   // of those, how many have no name
    int focused = 0;    // how many say the highlight is on them
    int texts = 0;      // plain words
};

bool controlRole(QAccessible::Role r)
{
    switch (r) {
    case QAccessible::Button:
    case QAccessible::CheckBox:
    case QAccessible::RadioButton:
    case QAccessible::Slider:
    case QAccessible::ComboBox:
    case QAccessible::EditableText:
    case QAccessible::ListItem:
    case QAccessible::MenuItem:
    case QAccessible::PageTab:
        return true;
    default:
        return false;
    }
}

void listen(QAccessibleInterface *at, int depth, Spoken &out, bool print)
{
    if (!at || !at->isValid() || depth > 64)
        return;
    const QAccessible::State st = at->state();
    if (st.invisible)
        return;
    const QAccessible::Role role = at->role();
    const QString name = at->text(QAccessible::Name);
    // The highlight also lands on words to read (so a long page scrolls): those count as having it too.
    if (st.focused)
        out.focused++;
    if (controlRole(role)) {
        out.controls++;
        if (name.trimmed().isEmpty())
            out.nameless++;
    } else if (role == QAccessible::StaticText && !name.isEmpty()) {
        out.texts++;
    }
    if (print && (controlRole(role) || st.focused || role == QAccessible::Dialog || role == QAccessible::PopupMenu || role == QAccessible::AlertMessage)) {
        const QByteArray line = QStringLiteral("SPOKEN role %1%2%3: %4 | %5\n")
                                    .arg(int(role))
                                    .arg(st.focused ? QStringLiteral(" focused") : QString())
                                    .arg(st.checkable ? (st.checked ? QStringLiteral(" checked") : QStringLiteral(" unchecked")) : QString())
                                    .arg(name, at->text(QAccessible::Description))
                                    .toUtf8();
        fputs(line.constData(), stdout);
    }
    const int n = at->childCount();
    for (int i = 0; i < n; i++)
        listen(at->child(i), depth + 1, out, print);
}
}

// what: 0 the number of controls a screen reader finds in the window, 1 how many of them have no name, 2 how many
// say the highlight is on them, 3 the number of plain texts, 4 the same as 0 with every control printed
// ("SPOKEN role ...: name | description"). Returns -1 when the window isn't there or has nothing to say.
extern "C" int turbinehub_test_spoken(int what)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    QAccessible::setActive(true);
    QAccessibleInterface *root = QAccessible::queryAccessibleInterface(win);
    if (!root)
        return -1;
    Spoken heard;
    listen(root, 0, heard, what == 4);
    if (what == 4)
        fflush(stdout);
    return what == 1 ? heard.nameless : what == 2 ? heard.focused : what == 3 ? heard.texts : heard.controls;
}

// The controller pointer (pointer.rs, Main.qml): mouse and wheel events made from the right stick and R3, delivered
// straight into the window the way a real mouse's are, so every tap handler, hover and list reacts to them and the
// clicks pass the same gate as a finger's. The system cursor isn't moved: gamescope (Game Mode) doesn't let an app
// move it, and there is none to see there. kind: 0 move, 1 press, 2 release, 3 wheel. button and buttons use Qt's
// values (1 left, 2 right). wheel: steps, positive scrolls down. Returns 1 when sent, -1 when the window isn't there.
extern "C" int turbinehub_pointer_event(int kind, double x, double y, int button, int buttons, int wheel)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    const QPointF local(x, y);
    const QPointF global = win->mapToGlobal(local);
    const Qt::MouseButtons held = Qt::MouseButtons::fromInt(buttons);
    if (kind == 3) {
        QWheelEvent ev(local, global, QPoint(), QPoint(0, -120 * wheel), held, Qt::NoModifier, Qt::NoScrollPhase, false);
        ev.setTimestamp(stamp());
        QCoreApplication::sendEvent(win, &ev);
        return 1;
    }
    const QEvent::Type type = kind == 1 ? QEvent::MouseButtonPress : kind == 2 ? QEvent::MouseButtonRelease : QEvent::MouseMove;
    QMouseEvent ev(type, local, global, kind == 0 ? Qt::NoButton : static_cast<Qt::MouseButton>(button), held, Qt::NoModifier);
    ev.setTimestamp(stamp());
    QCoreApplication::sendEvent(win, &ev);
    return 1;
}

// The window's size, for the pointer's speed and edges. Returns 1 when the window is there.
extern "C" int turbinehub_window_size(double *w, double *h)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    *w = win->width();
    *h = win->height();
    return 1;
}

// key: Qt's key code. mods: Qt's keyboard modifiers. repeat: the keyboard repeating a key that is held down.
// Returns 1 when sent, -1 when the window isn't there.
extern "C" int turbinehub_test_key(int key, int mods, int pressed, int repeat)
{
    QWindow *win = mainWindow();
    if (!win)
        return -1;
    // A key that types something carries what it types, as a real keyboard's does (a letter, a digit, a sign).
    const bool types = key >= 0x20 && key < 0x7f && (mods & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)) == 0;
    const QString text = types ? QString(QChar(key)).toLower() : QString();
    QKeyEvent ev(pressed ? QEvent::KeyPress : QEvent::KeyRelease, key, Qt::KeyboardModifiers::fromInt(mods), text, repeat != 0);
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
