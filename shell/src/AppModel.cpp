#include <QRegularExpression>
#include "AppModel.h"
#include <QDir>
#include <QProcess>
#include <QSettings>
#include <QStandardPaths>
#include <algorithm>

AppModel::AppModel(QObject *parent) : QAbstractListModel(parent) { reload(); }

QHash<int, QByteArray> AppModel::roleNames() const {
    return {{NameRole, "name"}, {IconRole, "icon"}, {ExecRole, "exec"}, {IdRole, "appId"}};
}
QVariant AppModel::data(const QModelIndex &i, int role) const {
    if (!i.isValid() || i.row() >= m_apps.size()) return {};
    const App &a = m_apps[i.row()];
    switch (role) {
    case NameRole: return a.name;
    case IconRole: return a.icon;
    case ExecRole: return a.exec;
    case IdRole: return a.id;
    }
    return {};
}

void AppModel::reload() {
    beginResetModel();
    m_apps.clear();
    QStringList dirs = QStandardPaths::standardLocations(QStandardPaths::ApplicationsLocation);
    const QString extra = qEnvironmentVariable("FLOWDE_APP_DIRS");  // test hook, ':'-separated
    if (!extra.isEmpty()) dirs = extra.split(':');
    QSet<QString> seen;
    for (const QString &d : dirs) {
        const auto files = QDir(d).entryInfoList({"*.desktop"}, QDir::Files);
        for (const auto &fi : files) {
            if (seen.contains(fi.fileName())) continue;
            QSettings s(fi.absoluteFilePath(), QSettings::IniFormat);
            s.beginGroup("Desktop Entry");
            if (s.value("Type").toString() != "Application") continue;
            if (s.value("NoDisplay").toBool() || s.value("Hidden").toBool()) continue;
            QString name = s.value("Name").toString();
            name.replace(QRegularExpression("fede", QRegularExpression::CaseInsensitiveOption), "Glaze");  // renamed project; never show the old name
            QString exec = s.value("Exec").toString();
            if (name.isEmpty() || exec.isEmpty()) continue;
            seen.insert(fi.fileName());
            m_apps.push_back({fi.completeBaseName(), name, s.value("Icon").toString(), exec});
        }
    }
    if (qEnvironmentVariable("FLOWDE_MOCK", "1") != "0") {
        static const char *mocks[] = {"Messages","Phone","Camera","Photos","Calendar","Clock","Music","Maps","Notes",
            "Weather","Wallet","Files","Noxal","Settings","Store","Browser","Contacts","Podcasts"};
        for (const char *m : mocks) {
            bool have = false;
            for (const App &a : m_apps) if (a.name == m) have = true;
            if (!have) m_apps.push_back({QString("mock.") + m, m, "", ""});
        }
    }
    std::sort(m_apps.begin(), m_apps.end(), [](const App &a, const App &b) {
        return a.name.localeAwareCompare(b.name) < 0; });
    endResetModel();
    emit countChanged();
}

bool AppModel::launch(int row) {
    if (row < 0 || row >= m_apps.size()) return false;
    QString exec = m_apps[row].exec;
    exec.replace(QRegularExpression("%[a-zA-Z]"), "");
    return QProcess::startDetached("/bin/sh", {"-c", exec.trimmed()});
}

QVariantMap AppModel::get(int row) const {
    if (row < 0 || row >= m_apps.size()) return {};
    const App &a = m_apps[row];
    return {{"name", a.name}, {"icon", a.icon}, {"exec", a.exec}, {"appId", a.id}};
}
