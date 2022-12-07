scalaVersion := "2.13.8"
name := "scala-defref-extractor"
organization := "io.bazeltools"
version := "1.0"

libraryDependencies += "org.scala-lang.modules" %% "scala-parser-combinators" % "2.1.1"
libraryDependencies += "com.monovore" %% "decline" % "2.2.0"
libraryDependencies += "org.scalameta" %% "scalameta" % "4.5.0"
libraryDependencies += "org.typelevel" %% "cats-effect" % "3.3.8"
libraryDependencies += "org.typelevel" %% "cats-parse" % "0.3.6"
libraryDependencies += "io.circe" %% "circe-core" % "0.14.3"
libraryDependencies += "io.circe" %% "circe-generic" % "0.14.3"
libraryDependencies += "com.github.javaparser" % "javaparser-core" % "3.24.8"

scalacOptions += "-deprecation"
scalacOptions += "-Wunused"

mainClass in Compile := Some("io.bazeltools.buildgen.scaladefref.Main")

enablePlugins(NativeImagePlugin)
nativeImageVersion := "22.3.0"
nativeImageJvm := "graalvm-java17"

nativeImageOptions += s"-H:ReflectionConfigurationFiles=${baseDirectory.value / "native-image-configs" / "reflect-config.json"}"
nativeImageOptions += s"-H:ConfigurationFileDirectories=${baseDirectory.value / "native-image-configs"}"
nativeImageOptions += "-H:+JNI"
