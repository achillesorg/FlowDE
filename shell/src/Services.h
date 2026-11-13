#pragma once
#include <QAbstractListModel>
#include <QLocalServer>
#include <QLocalSocket>
#include <QObject>
#include <QTimer>
#include <QVariantMap>

// Notifications: mock by default; kiln-notifs bridge TODO (D-Bus org.freedesktop.Notifications
// served by agent F's kiln-notifs). Set FLOWDE_MOCK=0 once that service exists.
class NotifModel : public QAbstractListModel {
    Q_OBJECT
public:
    enum Roles { AppRole = Qt::UserRole + 1, TitleRole, BodyRole, TimeRole };
    explicit NotifModel(QObject *p = nullptr);
    int rowCount(const QModelIndex & = {}) const override { return m_items.size(); }
    QVariant data(const QModelIndex &i, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE void dismiss(int row);
    Q_INVOKABLE void clear();
    Q_INVOKABLE void add(const QString &app, const QString &title, const QString &body);
private:
    struct N { QString app, title, body, time; };
    QVector<N> m_items;
};

// Mock/real device services plus the shell control socket (JSON lines).
// The compositor connects as a client and sends {"event":"gesture","name":"home"} etc.
class Services : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool wifi MEMBER m_wifi NOTIFY changed)
    Q_PROPERTY(bool bluetooth MEMBER m_bt NOTIFY changed)
    Q_PROPERTY(bool airplane MEMBER m_airplane NOTIFY changed)
    Q_PROPERTY(bool rotationLock MEMBER m_rotLock NOTIFY changed)
    Q_PROPERTY(bool dnd MEMBER m_dnd NOTIFY changed)
    Q_PROPERTY(double brightness MEMBER m_brightness NOTIFY changed)
    Q_PROPERTY(double volume MEMBER m_volume NOTIFY changed)
    Q_PROPERTY(int battery READ battery NOTIFY changed)
    Q_PROPERTY(bool charging MEMBER m_charging NOTIFY changed)
    Q_PROPERTY(int signal MEMBER m_signal NOTIFY changed)
    Q_PROPERTY(QString glassQuality MEMBER m_glass NOTIFY changed)
    Q_PROPERTY(bool reduceTransparency MEMBER m_reduceTransp NOTIFY changed)
    Q_PROPERTY(bool reduceMotion MEMBER m_reduceMotion NOTIFY changed)
    Q_PROPERTY(NotifModel *notifications READ notifications CONSTANT)
    Q_PROPERTY(bool mock READ mock CONSTANT)
public:
    explicit Services(QObject *p = nullptr);
    NotifModel *notifications() { return &m_notifs; }
    int battery() const { return m_battery; }
    bool mock() const { return m_mock; }
    Q_INVOKABLE void send(const QString &json);   // shell -> compositor
signals:
    void changed();
    void gesture(const QString &name);            // home|back|switcher|shade|qs|rotate
    void command(const QString &key, const QString &value);  // test/theme hook
private:
    void onLine(const QByteArray &line);
    void pollBattery();
    bool m_wifi = true, m_bt = true, m_airplane = false, m_rotLock = true, m_dnd = false;
    double m_brightness = 0.7, m_volume = 0.45;
    int m_battery = 78, m_signal = 3;
    bool m_charging = false, m_mock = true, m_reduceTransp = false, m_reduceMotion = false;
    QString m_glass = "full";
    NotifModel m_notifs;
    QLocalServer m_server;
    QList<QLocalSocket *> m_clients;
    QTimer m_battTimer;
};
