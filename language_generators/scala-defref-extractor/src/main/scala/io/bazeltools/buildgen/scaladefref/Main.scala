package io.bazeltools.buildgen.scaladefref

import cats.effect.{IO}
import io.bazeltools.buildgen.shared.DataBlock
import io.bazeltools.buildgen.shared.DriverApplication


object Main extends DriverApplication {
  def name: String = "scala_extractor"
  def extract(data: String): IO[DataBlock] = {
    ScalaSourceEntityExtractor.extract(data)
  }
}
