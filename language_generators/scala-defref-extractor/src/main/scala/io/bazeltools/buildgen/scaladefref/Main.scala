package io.bazeltools.buildgen.scaladefref

import cats.effect.IO
import io.bazeltools.buildgen.shared.{DriverApplication, Entity, Symbols}

object Main extends DriverApplication {
  def name: String = "scala_extractor"
  def extract(data: String, specialTlds: Set[String]): IO[Symbols] = {
    val map = Entity.makeSpecialTldsMap(specialTlds)
    ScalaSourceEntityExtractor(map).extract(data)
  }
}
