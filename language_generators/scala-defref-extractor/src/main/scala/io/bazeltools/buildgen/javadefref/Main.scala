package io.bazeltools.buildgen.javadefref

import cats.effect.IO
import io.bazeltools.buildgen.shared.{Symbols, DriverApplication}

object Main extends DriverApplication {
  def name: String = "java_extractor"
  def extract(data: String, _specialTlds: Set[String]): IO[Symbols] =
    JavaSourceEntityExtractor.extract(data)
}
