// What the Rust side can't reach through cxx-qt-lib: the window's identity, so Desktop Mode's taskbar shows
// Turbine Hub's name and icon instead of a generic one, and Qt's message handler (see the end of this file).

#include <QtCore/QFile>
#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

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
