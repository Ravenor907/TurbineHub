// What the Rust side can't reach through cxx-qt-lib: the window's identity, so Desktop Mode's taskbar shows
// Turbine Hub's name and icon instead of a generic one.

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
