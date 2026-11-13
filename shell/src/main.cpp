#include <QElapsedTimer>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include "AppModel.h"
#include "Services.h"

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName("flowde-shell");
    Services services;
    AppModel apps;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("Services", &services);
    engine.rootContext()->setContextProperty("Apps", &apps);
    engine.rootContext()->setContextProperty("StartScreen", qEnvironmentVariable("FLOWDE_START", "lock"));
    engine.rootContext()->setContextProperty("StartTheme", qEnvironmentVariable("FLOWDE_THEME", "LiquidGlass"));
    engine.rootContext()->setContextProperty("StartDark", qEnvironmentVariable("FLOWDE_SCHEME", "light") == "dark");
    engine.loadFromModule("FlowShell", "Main");
    if (engine.rootObjects().isEmpty()) return 1;

    // frame-time stats (FLOWDE_FPS=1): print avg/max ms per 120 frames of continuous rendering
    if (qEnvironmentVariable("FLOWDE_FPS") == "1") {
        auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        static QElapsedTimer t; static qint64 last = 0, sum = 0, mx = 0; static int n = 0;
        t.start();
        QObject::connect(w, &QQuickWindow::frameSwapped, w, [] {
            qint64 now = t.nsecsElapsed(); if (last) { qint64 d = now - last; sum += d; mx = qMax(mx, d); ++n; }
            last = now;
            if (n == 120) { qInfo("frametime avg %.1f ms max %.1f ms (%d frames)", sum / n / 1e6, mx / 1e6, n); sum = mx = 0; n = 0; }
        });
    }
    return app.exec();
}
