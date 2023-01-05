package io.bazeltools.buildgen.shared

import cats.effect.{ExitCode, IO, IOApp}

import java.nio.file.Path
import com.monovore.decline.{Command, Opts}
import com.monovore.decline
import cats.implicits._
import io.circe.Encoder
import java.nio.file.Files
import java.nio.charset.StandardCharsets
import io.circe.syntax.EncoderOps

abstract class DriverApplication extends IOApp {
  def name: String
  def extract(data: String): IO[Symbols]

  def readToString(path: Path): IO[String] =
    IO.blocking(
      new String(Files.readAllBytes(path), StandardCharsets.UTF_8)
    )

  def writeJson[A: Encoder](path: Path, value: => A): IO[Unit] =
    IO.blocking {
      val _ = Files.write(
        path,
        value.asJson.noSpacesSortKeys.getBytes(StandardCharsets.UTF_8)
      )
    }

     private[this] def extractDataBlocks(
      workingDirectory: Path,
      paths: String*
  ): IO[List[DataBlock]] = {
    paths.sorted.toList.traverse { path =>
      for {
        content <- readToString(workingDirectory.resolve(path))
        try_e <- extract(content).attempt
        e <- try_e match {
          case Right(x) => IO.pure(x)
          case Left(f) =>
            IO.raiseError(
              new Exception(
                s"Failed in parsing of ${workingDirectory.resolve(path)}, with error\n$f"
              )
            )
        }
      } yield e.withEntityPath(path)
    }
  }
  def main: Command[IO[ExitCode]] = decline.Command(
    name,
    "Extract definitions and references from source files"
  ) {
    (
      Opts.option[String]("relative-input-paths", "input files to process"),
      Opts.option[Path]("working-directory", "input files to process"),
      Opts.option[String]("label-or-repo-path", "label to assign these files"),
      Opts.option[Path]("output", "target location to write to")
    ).mapN { (inputs, workingDirectory, label_or_repo_path, outputPath) =>
      IO.pure(ExitCode.Success)
      for {
        dataBlocks <- extractDataBlocks(
          workingDirectory,
          inputs.split(',').toList: _*
        )
        extractedData = ExtractedData(
          label_or_repo_path = label_or_repo_path,
          data_blocks = dataBlocks
        )
        _ <- writeJson(outputPath, extractedData)
      } yield ExitCode.Success
    }
  }

  def run(args: List[String]): IO[ExitCode] =
    IO(main.parse(args)).flatMap {
      case Right(exitCode) => exitCode
      case Left(err) =>
        IO {
          System.err.println(err.toString)
          ExitCode.Error
        }
    }
}
