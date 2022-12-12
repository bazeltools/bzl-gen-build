package io.bazeltools.buildgen.javadefref

import cats.effect.{IO}
import io.bazeltools.buildgen.shared.DataBlock
import io.bazeltools.buildgen.shared.DriverApplication

object Main extends DriverApplication {
  def name: String = "java_extractor"
  def extract(data: String): IO[DataBlock] = {
    JavaSourceEntityExtractor.extract(data)
  }
}
