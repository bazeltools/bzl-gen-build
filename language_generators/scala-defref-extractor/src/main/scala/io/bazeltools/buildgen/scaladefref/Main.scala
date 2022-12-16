package io.bazeltools.buildgen.scaladefref

import cats.effect.{IO}
import io.bazeltools.buildgen.shared.{Symbols, DriverApplication}

object Main extends DriverApplication {
  def name: String = "scala_extractor"
  def extract(data: String): IO[Symbols] =
    ScalaSourceEntityExtractor.extract(data)
}
