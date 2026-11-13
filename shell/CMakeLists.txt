cmake_minimum_required(VERSION 3.21)
project(flowde-shell VERSION 0.1.0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick Network ShaderTools)
qt_standard_project_setup(REQUIRES 6.5)

qt_add_executable(flowde-shell src/main.cpp src/Services.cpp src/Services.h src/AppModel.cpp src/AppModel.h)

set(QML_FILES qml/Main.qml qml/Theme.qml qml/GlassPanel.qml qml/Wallpaper.qml qml/StatusBar.qml
  qml/GestureBar.qml qml/LockScreen.qml qml/HomeScreen.qml qml/AppSwitcher.qml qml/Shade.qml
  qml/AppIcon.qml qml/Toggle.qml qml/SliderRow.qml)
set_source_files_properties(qml/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(flowde-shell URI FlowShell VERSION 1.0 QML_FILES ${QML_FILES} NO_RESOURCE_TARGET_PATH)
qt_add_shaders(flowde-shell "flowde-shaders" PREFIX "/FlowShell" FILES shaders/glass.frag shaders/wallpaper.frag
  OUTPUTS shaders/glass.frag.qsb shaders/wallpaper.frag.qsb BATCHABLE
  GLSL "100 es,120,150" NOHLSL NOMSL)
target_link_libraries(flowde-shell PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::Network)
install(TARGETS flowde-shell RUNTIME DESTINATION bin)
