#pragma once
#include <QAbstractListModel>
#include <QString>
#include <QVariantMap>
#include <QVector>

// App grid model built from XDG .desktop files (Name, Exec, Icon, NoDisplay).
class AppModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles { NameRole = Qt::UserRole + 1, IconRole, ExecRole, IdRole };
    explicit AppModel(QObject *parent = nullptr);
    int rowCount(const QModelIndex & = {}) const override { return m_apps.size(); }
    QVariant data(const QModelIndex &idx, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE void reload();
    Q_INVOKABLE bool launch(int row);
    Q_INVOKABLE QVariantMap get(int row) const;
signals:
    void countChanged();
private:
    struct App { QString id, name, icon, exec; };
    QVector<App> m_apps;
};
