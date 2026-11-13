#include "Services.h"
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>

NotifModel::NotifModel(QObject *p) : QAbstractListModel(p) {
    m_items = {
        {"Messages", "Mara", "Are we still on for tonight? I booked the table.", "now"},
        {"Calendar", "Design review in 15 min", "Room 4 / video link attached", "5m"},
        {"Noxal", "Summary ready", "3 unread threads condensed into 4 lines.", "12m"},
        {"System", "Update available", "FlowDE 0.2 prototype build is ready to install.", "1h"},
    };
}
QHash<int, QByteArray> NotifModel::roleNames() const {
    return {{AppRole, "app"}, {TitleRole, "title"}, {BodyRole, "body"}, {TimeRole, "time"}};
}
QVariant NotifModel::data(const QModelIndex &i, int r) const {
    if (!i.isValid() || i.row() >= m_items.size()) return {};
    const N &n = m_items[i.row()];
    switch (r) { case AppRole: return n.app; case TitleRole: return n.title;
                 case BodyRole: return n.body; case TimeRole: return n.time; }
    return {};
}
void NotifModel::dismiss(int row) {
    if (row < 0 || row >= m_items.size()) return;
    beginRemoveRows({}, row, row); m_items.removeAt(row); endRemoveRows();
}
void NotifModel::clear() { beginResetModel(); m_items.clear(); endResetModel(); }
void NotifModel::add(const QString &a, const QString &t, const QString &b) {
    beginInsertRows({}, 0, 0); m_items.prepend({a, t, b, "now"}); endInsertRows();
}

Services::Services(QObject *p) : QObject(p) {
    m_mock = qEnvironmentVariable("FLOWDE_MOCK", "1") != "0";
    m_glass = qEnvironmentVariable("FLOWDE_GLASS", "full");
    m_reduceTransp = qEnvironmentVariable("FLOWDE_REDUCE_TRANSPARENCY") == "1";
    m_reduceMotion = qEnvironmentVariable("FLOWDE_REDUCE_MOTION") == "1";
    pollBattery();
    connect(&m_battTimer, &QTimer::timeout, this, &Services::pollBattery);
    m_battTimer.start(30000);

    QString path = qEnvironmentVariable("FLOWDE_SHELL_SOCKET");
    if (path.isEmpty()) {
        QString rt = qEnvironmentVariable("XDG_RUNTIME_DIR", "/tmp");
        path = rt + "/flowde-shell.sock";
    }
    QLocalServer::removeServer(path);
    if (m_server.listen(path)) qInfo() << "flowde-shell control socket" << path;
    connect(&m_server, &QLocalServer::newConnection, this, [this] {
        while (auto *s = m_server.nextPendingConnection()) {
            m_clients << s;
            connect(s, &QLocalSocket::readyRead, this, [this, s] {
                while (s->canReadLine()) onLine(s->readLine());
            });
            connect(s, &QLocalSocket::disconnected, this, [this, s] { m_clients.removeOne(s); s->deleteLater(); });
        }
    });
}

void Services::pollBattery() {
    const QDir d("/sys/class/power_supply");
    for (const QString &n : d.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        QFile cap(d.filePath(n + "/capacity"));
        if (n.startsWith("BAT") && cap.open(QIODevice::ReadOnly)) {
            m_battery = cap.readAll().trimmed().toInt();
            QFile st(d.filePath(n + "/status"));
            if (st.open(QIODevice::ReadOnly)) m_charging = st.readAll().trimmed() == "Charging";
            emit changed();
            return;
        }
    }
    emit changed();  // no battery: keep mock value
}

void Services::send(const QString &json) {
    for (auto *c : std::as_const(m_clients)) c->write(json.toUtf8() + "\n");
}

// Protocol (JSON lines):
//   {"event":"gesture","name":"home|back|switcher|shade|qs"}
//   {"set":"theme|scheme|glass|reduceTransparency|reduceMotion|screen","value":"..."}
//   {"notify":{"app":..,"title":..,"body":..}}
void Services::onLine(const QByteArray &line) {
    const auto o = QJsonDocument::fromJson(line).object();
    if (o["event"].toString() == "gesture") emit gesture(o["name"].toString());
    else if (o.contains("set")) {
        const QString k = o["set"].toString(), v = o["value"].toVariant().toString();
        if (k == "glass") { m_glass = v; emit changed(); }
        else if (k == "reduceTransparency") { m_reduceTransp = v == "1" || v == "true"; emit changed(); }
        else if (k == "reduceMotion") { m_reduceMotion = v == "1" || v == "true"; emit changed(); }
        emit command(k, v);
    } else if (o.contains("notify")) {
        const auto n = o["notify"].toObject();
        m_notifs.add(n["app"].toString(), n["title"].toString(), n["body"].toString());
    }
}
